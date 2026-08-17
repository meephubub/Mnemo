//! DOCX (OOXML WordprocessingML) text extraction (plan.md Phase 2
//! "Document Ingestion" — DOCX parsing).
//!
//! A `.docx` file is a ZIP container; the document body lives in
//! `word/document.xml` as a flat run of `<w:p>` (paragraph) elements
//! containing `<w:r>` (run) elements containing `<w:t>` (text) nodes.
//! Rather than pull in a full XML/DOM dependency, this module does a
//! single linear scan over that XML looking only for the handful of
//! tags that matter for plain-text extraction — in the same spirit as
//! `parsers::html`'s dependency-free tag stripper.
//!
//! Paragraphs whose style is one of Word's built-in heading styles
//! (`Heading1`..`Heading9`, `Title`) become section headings, mirroring
//! how `parsers::markdown` turns `#`-headings into sections.

use std::io::Read;

use crate::error::{IngestError, Result};
use crate::parsers::ParsedSection;

/// Extract the document title (from `docProps/core.xml`'s `dc:title`,
/// falling back to a `Title`-styled paragraph if present) and split
/// the body into heading-delimited sections.
pub fn parse(raw: &[u8]) -> Result<(Option<String>, Vec<ParsedSection>)> {
    let cursor = std::io::Cursor::new(raw);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| IngestError::Parse(format!("invalid DOCX archive: {e}")))?;

    let core_title = read_core_title(&mut archive);
    let document_xml = read_entry_utf8(&mut archive, "word/document.xml")?;
    let paragraphs = extract_paragraphs(&document_xml);

    Ok(group_into_sections(paragraphs, core_title))
}

/// A single `<w:p>` paragraph's style (if any Word heading style was
/// applied) and its plain-text content.
struct RawParagraph {
    style_val: Option<String>,
    text: String,
}

fn is_heading_style(style_val: &str) -> bool {
    let lower = style_val.to_lowercase();
    lower.starts_with("heading") || lower == "title"
}

fn group_into_sections(
    paragraphs: Vec<RawParagraph>,
    core_title: Option<String>,
) -> (Option<String>, Vec<ParsedSection>) {
    let mut sections = Vec::new();
    let mut title = core_title;

    let mut current_heading: Option<String> = None;
    let mut current_text = String::new();
    let mut offset = 0usize;

    for para in paragraphs {
        let is_heading = para.style_val.as_deref().map(is_heading_style).unwrap_or(false);

        if is_heading {
            if !current_text.trim().is_empty() || current_heading.is_some() {
                let len = current_text.len();
                sections.push(ParsedSection {
                    heading: current_heading.take(),
                    text: std::mem::take(&mut current_text),
                    start_offset: offset,
                    page: None,
                });
                // Matches the "\n" joiner used to build
                // `ParsedFile::text` for DOCX in `parsers::parse`.
                offset += len + 1;
            }

            let heading_text = para.text.trim().to_string();
            let is_title_style = para
                .style_val
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("title"))
                .unwrap_or(false);
            if title.is_none() && is_title_style && !heading_text.is_empty() {
                title = Some(heading_text.clone());
            }
            current_heading = Some(heading_text);
        } else {
            let trimmed = para.text.trim();
            if !trimmed.is_empty() {
                if !current_text.is_empty() {
                    current_text.push_str("\n\n");
                }
                current_text.push_str(trimmed);
            }
        }
    }

    if !current_text.trim().is_empty() || current_heading.is_some() {
        sections.push(ParsedSection {
            heading: current_heading,
            text: current_text,
            start_offset: offset,
            page: None,
        });
    }

    if sections.is_empty() {
        sections.push(ParsedSection {
            heading: None,
            text: String::new(),
            start_offset: 0,
            page: None,
        });
    }

    (title, sections)
}

fn read_entry_utf8<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|e| IngestError::Parse(format!("DOCX missing {name}: {e}")))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| IngestError::Parse(format!("failed reading {name} from DOCX: {e}")))?;
    String::from_utf8(buf).map_err(|e| IngestError::Parse(format!("{name} is not valid UTF-8: {e}")))
}

fn read_core_title<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Option<String> {
    let xml = read_entry_utf8(archive, "docProps/core.xml").ok()?;
    extract_tag_text(&xml, "dc:title")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract the text content of the first `<tag>...</tag>` element
/// (namespaced tags like `dc:title` included). Good enough for the
/// handful of flat, single-value core-properties fields we care
/// about; not a general XML query.
fn extract_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let open_start = xml.find(&open)?;
    let open_tag_end = xml[open_start..].find('>')? + open_start + 1;
    let close = format!("</{tag}>");
    let close_rel = xml[open_tag_end..].find(&close)?;
    Some(decode_xml_entities(&xml[open_tag_end..open_tag_end + close_rel]))
}

fn tag_name(tag_str: &str) -> &str {
    tag_str
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
}

fn is_closing(tag_str: &str) -> bool {
    tag_str.trim_start().starts_with('/')
}

fn is_self_closing(tag_str: &str) -> bool {
    tag_str.trim_end().ends_with('/')
}

fn attr_value(tag_str: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{attr}={quote}");
        if let Some(pos) = tag_str.find(&needle) {
            let start = pos + needle.len();
            if let Some(end_rel) = tag_str[start..].find(quote) {
                return Some(tag_str[start..start + end_rel].to_string());
            }
        }
    }
    None
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Linear scan over `word/document.xml` extracting each `<w:p>`
/// paragraph's heading style (if any) and plain text, honoring
/// `<w:tab/>` and `<w:br/>`/`<w:cr/>` as whitespace within a
/// paragraph's runs.
fn extract_paragraphs(xml: &str) -> Vec<RawParagraph> {
    let mut paragraphs = Vec::new();
    let mut in_paragraph = false;
    let mut current_style: Option<String> = None;
    let mut current_text = String::new();
    let mut inside_wt = false;

    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        let text_node = &rest[..lt];
        if inside_wt && in_paragraph && !text_node.is_empty() {
            current_text.push_str(&decode_xml_entities(text_node));
        }
        rest = &rest[lt + 1..];

        let gt = match rest.find('>') {
            Some(g) => g,
            None => break, // malformed trailing tag; stop rather than panic
        };
        let tag_str = &rest[..gt];
        rest = &rest[gt + 1..];

        let name = tag_name(tag_str);
        let closing = is_closing(tag_str);
        let self_closing = is_self_closing(tag_str);

        match name {
            "w:p" => {
                if closing {
                    if in_paragraph {
                        paragraphs.push(RawParagraph {
                            style_val: current_style.take(),
                            text: std::mem::take(&mut current_text),
                        });
                        in_paragraph = false;
                    }
                } else {
                    // Defensive flush in case of malformed/unbalanced
                    // input; `<w:p>` never legitimately nests.
                    if in_paragraph {
                        paragraphs.push(RawParagraph {
                            style_val: current_style.take(),
                            text: std::mem::take(&mut current_text),
                        });
                    }
                    in_paragraph = true;
                    current_style = None;
                    current_text.clear();
                    if self_closing {
                        paragraphs.push(RawParagraph {
                            style_val: None,
                            text: String::new(),
                        });
                        in_paragraph = false;
                    }
                }
            }
            "w:pStyle" if in_paragraph => {
                if let Some(val) = attr_value(tag_str, "w:val") {
                    current_style = Some(val);
                }
            }
            "w:t" => {
                inside_wt = !closing && !self_closing;
            }
            "w:tab" if in_paragraph => current_text.push('\t'),
            "w:br" | "w:cr" if in_paragraph => current_text.push('\n'),
            _ => {}
        }
    }

    if in_paragraph {
        paragraphs.push(RawParagraph {
            style_val: current_style,
            text: current_text,
        });
    }

    paragraphs
}

/// Test-only helpers for building minimal, valid `.docx` bytes in
/// pure Rust (no external tools, no fixture files). `pub(crate)` so
/// both this module's own tests and `mnemo-ingest`'s crate-level
/// integration tests can build DOCX fixtures.
#[cfg(test)]
pub(crate) mod test_support {
    use std::io::Write;

    /// Builds a minimal in-memory `.docx` (a ZIP containing just
    /// `word/document.xml` and, optionally, `docProps/core.xml`).
    /// This is enough for `docx::parse`, which never looks at
    /// `[Content_Types].xml` or `_rels/` (only real Word does).
    pub(crate) fn build_test_docx(document_xml: &str, core_xml: Option<&str>) -> Vec<u8> {
        let mut zip_bytes = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut zip_bytes);
            let mut writer = zip::ZipWriter::new(cursor);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

            writer.start_file("word/document.xml", options).unwrap();
            writer.write_all(document_xml.as_bytes()).unwrap();

            if let Some(core) = core_xml {
                writer.start_file("docProps/core.xml", options).unwrap();
                writer.write_all(core.as_bytes()).unwrap();
            }

            writer.finish().unwrap();
        }
        zip_bytes
    }

    pub(crate) const DOC_NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
}

#[cfg(test)]
mod tests {
    use super::test_support::{build_test_docx, DOC_NS};
    use super::*;

    #[test]
    fn extracts_plain_paragraphs_with_no_headings() {
        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:r><w:t>First paragraph.</w:t></w:r></w:p>
                <w:p><w:r><w:t>Second paragraph.</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let bytes = build_test_docx(&xml, None);
        let (title, sections) = parse(&bytes).unwrap();
        assert_eq!(title, None);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].heading.is_none());
        assert!(sections[0].text.contains("First paragraph."));
        assert!(sections[0].text.contains("Second paragraph."));
    }

    #[test]
    fn heading_styled_paragraph_starts_a_new_section() {
        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Intro</w:t></w:r></w:p>
                <w:p><w:r><w:t>Intro body text.</w:t></w:r></w:p>
                <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Details</w:t></w:r></w:p>
                <w:p><w:r><w:t>Details body text.</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let bytes = build_test_docx(&xml, None);
        let (_, sections) = parse(&bytes).unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].heading.as_deref(), Some("Intro"));
        assert!(sections[0].text.contains("Intro body text."));
        assert_eq!(sections[1].heading.as_deref(), Some("Details"));
        assert!(sections[1].text.contains("Details body text."));
    }

    #[test]
    fn title_style_paragraph_sets_document_title() {
        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>My Document</w:t></w:r></w:p>
                <w:p><w:r><w:t>Body text.</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let bytes = build_test_docx(&xml, None);
        let (title, sections) = parse(&bytes).unwrap();
        assert_eq!(title.as_deref(), Some("My Document"));
        assert_eq!(sections[0].heading.as_deref(), Some("My Document"));
    }

    #[test]
    fn core_properties_title_takes_priority_over_title_style() {
        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:r><w:t>Body text.</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let core = r#"<cp:coreProperties xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Core Title</dc:title></cp:coreProperties>"#;
        let bytes = build_test_docx(&xml, Some(core));
        let (title, _) = parse(&bytes).unwrap();
        assert_eq!(title.as_deref(), Some("Core Title"));
    }

    #[test]
    fn tab_and_line_break_runs_become_whitespace() {
        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:r><w:t>Col1</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>Col2</w:t></w:r>
                <w:r><w:br/></w:r><w:r><w:t>Next line</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let bytes = build_test_docx(&xml, None);
        let (_, sections) = parse(&bytes).unwrap();
        assert!(sections[0].text.contains("Col1\tCol2"));
        assert!(sections[0].text.contains("\nNext line"));
    }

    #[test]
    fn empty_document_yields_one_empty_section() {
        let xml = format!(r#"<w:document {DOC_NS}><w:body></w:body></w:document>"#);
        let bytes = build_test_docx(&xml, None);
        let (title, sections) = parse(&bytes).unwrap();
        assert_eq!(title, None);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].text.is_empty());
    }

    #[test]
    fn xml_entities_in_text_runs_are_decoded() {
        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:r><w:t>Fish &amp; chips &lt;tasty&gt;</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let bytes = build_test_docx(&xml, None);
        let (_, sections) = parse(&bytes).unwrap();
        assert!(sections[0].text.contains("Fish & chips <tasty>"));
    }

    #[test]
    fn not_a_zip_archive_produces_a_parse_error() {
        let result = parse(b"definitely not a docx");
        assert!(result.is_err());
    }
}
