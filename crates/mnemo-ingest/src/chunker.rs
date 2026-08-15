//! Configurable, content-aware chunking (plan.md section 13
//! "Chunking").
//!
//! Chunking here is paragraph-aware within each of a document's
//! sections (a Markdown section, or the whole file for formats
//! without sections), and packs consecutive paragraphs up to a
//! target character budget rather than cutting mid-sentence.

use crate::parsers::ParsedFile;

#[derive(Debug, Clone)]
pub struct ChunkConfig {
    /// Soft upper bound on characters per chunk. A single paragraph
    /// longer than this is kept whole rather than split further.
    pub target_chars: usize,
    /// Minimum chunk size before we bother emitting it standalone;
    /// small trailing paragraphs are merged into the previous chunk
    /// when possible to avoid tiny, low-value chunks.
    pub min_chars: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_chars: 1200,
            min_chars: 200,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkDraft {
    pub text: String,
    pub section: Option<String>,
    pub start_offset: usize,
    pub end_offset: usize,
}

pub fn chunk_parsed_file(parsed: &ParsedFile, config: &ChunkConfig) -> Vec<ChunkDraft> {
    let mut drafts = Vec::new();

    for (heading, section_text, section_start) in &parsed.sections {
        drafts.extend(chunk_section(heading.clone(), section_text, *section_start, config));
    }

    drafts
}

fn chunk_section(
    heading: Option<String>,
    text: &str,
    section_start: usize,
    config: &ChunkConfig,
) -> Vec<ChunkDraft> {
    let paragraphs: Vec<(&str, usize)> = split_paragraphs_with_offsets(text);
    let mut drafts: Vec<ChunkDraft> = Vec::new();

    let mut buf = String::new();
    let mut buf_start: Option<usize> = None;
    let mut buf_end = 0usize;

    for (para, local_offset) in paragraphs {
        if para.trim().is_empty() {
            continue;
        }

        let would_be_len = buf.len() + para.len() + 2;
        if !buf.is_empty() && would_be_len > config.target_chars {
            drafts.push(ChunkDraft {
                text: buf.trim().to_string(),
                section: heading.clone(),
                start_offset: section_start + buf_start.unwrap_or(0),
                end_offset: section_start + buf_end,
            });
            buf.clear();
            buf_start = None;
        }

        if buf_start.is_none() {
            buf_start = Some(local_offset);
        }
        if !buf.is_empty() {
            buf.push_str("\n\n");
        }
        buf.push_str(para);
        buf_end = local_offset + para.len();
    }

    if !buf.trim().is_empty() {
        // Merge a too-small trailing chunk into the previous one so we
        // don't emit low-value slivers.
        if buf.len() < config.min_chars {
            if let Some(last) = drafts.last_mut() {
                last.text.push_str("\n\n");
                last.text.push_str(buf.trim());
                last.end_offset = section_start + buf_end;
            } else {
                drafts.push(ChunkDraft {
                    text: buf.trim().to_string(),
                    section: heading.clone(),
                    start_offset: section_start + buf_start.unwrap_or(0),
                    end_offset: section_start + buf_end,
                });
            }
        } else {
            drafts.push(ChunkDraft {
                text: buf.trim().to_string(),
                section: heading,
                start_offset: section_start + buf_start.unwrap_or(0),
                end_offset: section_start + buf_end,
            });
        }
    }

    drafts
}

/// Split on blank lines, returning each paragraph with its byte
/// offset relative to the start of `text`.
fn split_paragraphs_with_offsets(text: &str) -> Vec<(&str, usize)> {
    let mut result = Vec::new();
    let mut offset = 0usize;
    for part in text.split("\n\n") {
        if !part.trim().is_empty() {
            // Find the actual start of trimmed content within `part`.
            let leading_ws = part.len() - part.trim_start().len();
            result.push((part.trim(), offset + leading_ws));
        }
        offset += part.len() + 2; // account for the removed "\n\n"
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{FileKind, ParsedFile};

    fn parsed_single_section(text: &str) -> ParsedFile {
        ParsedFile {
            kind: FileKind::Text,
            title: None,
            text: text.to_string(),
            sections: vec![(None, text.to_string(), 0)],
        }
    }

    #[test]
    fn empty_text_produces_no_chunks() {
        let parsed = parsed_single_section("");
        let drafts = chunk_parsed_file(&parsed, &ChunkConfig::default());
        assert!(drafts.is_empty());
    }

    #[test]
    fn single_short_paragraph_is_one_chunk() {
        let parsed = parsed_single_section("Just one short paragraph.");
        let drafts = chunk_parsed_file(&parsed, &ChunkConfig::default());
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].text, "Just one short paragraph.");
        assert_eq!(drafts[0].start_offset, 0);
    }

    #[test]
    fn paragraphs_are_packed_up_to_target_chars() {
        // Three ~40-char paragraphs with a tiny target budget should
        // produce more than one chunk, each within reach of the budget.
        let text = format!("{}\n\n{}\n\n{}", "a".repeat(40), "b".repeat(40), "c".repeat(40));
        let parsed = parsed_single_section(&text);
        let config = ChunkConfig {
            target_chars: 50,
            min_chars: 10,
        };
        let drafts = chunk_parsed_file(&parsed, &config);
        assert!(drafts.len() >= 2, "expected multiple chunks, got {}", drafts.len());
        for d in &drafts {
            assert!(!d.text.trim().is_empty());
        }
    }

    #[test]
    fn tiny_trailing_paragraph_is_merged_into_previous_chunk() {
        let text = format!("{}\n\nshort", "x".repeat(60));
        let parsed = parsed_single_section(&text);
        let config = ChunkConfig {
            target_chars: 1000,
            min_chars: 20,
        };
        let drafts = chunk_parsed_file(&parsed, &config);
        assert_eq!(drafts.len(), 1);
        assert!(drafts[0].text.ends_with("short"));
    }

    #[test]
    fn blank_only_text_produces_no_chunks() {
        let parsed = parsed_single_section("\n\n   \n\n");
        let drafts = chunk_parsed_file(&parsed, &ChunkConfig::default());
        assert!(drafts.is_empty());
    }

    #[test]
    fn section_heading_is_carried_onto_chunks() {
        let parsed = ParsedFile {
            kind: FileKind::Markdown,
            title: None,
            text: "Some body text.".to_string(),
            sections: vec![(Some("Intro".to_string()), "Some body text.".to_string(), 0)],
        };
        let drafts = chunk_parsed_file(&parsed, &ChunkConfig::default());
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].section.as_deref(), Some("Intro"));
    }
}
