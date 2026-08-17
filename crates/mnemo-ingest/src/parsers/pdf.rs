//! PDF text extraction (plan.md Phase 2 "Document Ingestion" — PDF
//! parsing).
//!
//! Delegates the actual PDF object/content-stream parsing to
//! `pdf-extract`; this module's job is turning its per-page output
//! into [`ParsedSection`]s with 1-based page numbers so downstream
//! chunks can carry accurate page citations (`Chunk::page`).

use crate::error::{IngestError, Result};
use crate::parsers::ParsedSection;

/// Extract text from a PDF's raw bytes, one [`ParsedSection`] per
/// page. PDFs don't have a reliable "document title" field this
/// crate can extract without a full metadata parse, so the title is
/// always `None`; callers can fall back to a filename.
pub fn parse(raw: &[u8]) -> Result<(Option<String>, Vec<ParsedSection>)> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(raw)
        .map_err(|e| IngestError::Parse(format!("invalid PDF: {e}")))?;

    let mut sections = Vec::with_capacity(pages.len().max(1));
    let mut offset = 0usize;

    for (index, page_text) in pages.into_iter().enumerate() {
        let text = normalize(&page_text);
        let len = text.len();
        sections.push(ParsedSection {
            heading: None,
            text,
            start_offset: offset,
            page: Some((index + 1) as u32),
        });
        // Matches the "\n\n" joiner used to build `ParsedFile::text`
        // for PDFs in `parsers::parse`.
        offset += len + 2;
    }

    if sections.is_empty() {
        sections.push(ParsedSection {
            heading: None,
            text: String::new(),
            start_offset: 0,
            page: Some(1),
        });
    }

    Ok((None, sections))
}

/// `pdf-extract` preserves the raw layout-derived whitespace of a
/// page (including a trailing run of blank lines); trim that down to
/// something worth chunking/indexing.
fn normalize(raw: &str) -> String {
    raw.trim().to_string()
}

/// Test-only helpers for building minimal, valid PDF bytes in pure
/// Rust (no external tools, no fixture files). `pub(crate)` so both
/// this module's own tests and `mnemo-ingest`'s crate-level
/// integration tests can build PDF fixtures.
#[cfg(test)]
pub(crate) mod test_support {
    /// Builds a minimal, spec-valid single-page PDF (uncompressed,
    /// with an exact xref table) containing a single `Tj` text-showing
    /// operator.
    pub(crate) fn build_minimal_pdf(page_text: &str) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        let mut offsets: Vec<usize> = vec![0]; // placeholder for free object 0

        buf.extend_from_slice(b"%PDF-1.4\n");

        let mut push_obj = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, id: u32, body: &str| {
            offsets.push(buf.len());
            buf.extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
        };

        push_obj(&mut buf, &mut offsets, 1, "<< /Type /Catalog /Pages 2 0 R >>");
        push_obj(&mut buf, &mut offsets, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        push_obj(
            &mut buf,
            &mut offsets,
            3,
            "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> \
             /MediaBox [0 0 612 792] /Contents 5 0 R >>",
        );
        push_obj(&mut buf, &mut offsets, 4, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");

        let escaped = escape_pdf_text(page_text);
        let stream_content = format!("BT /F1 24 Tf 72 700 Td ({escaped}) Tj ET");
        let stream_obj = format!(
            "<< /Length {} >>\nstream\n{stream_content}\nendstream",
            stream_content.len() + 1
        );
        push_obj(&mut buf, &mut offsets, 5, &stream_obj);

        let xref_offset = buf.len();
        let object_count = offsets.len(); // includes free object 0
        buf.extend_from_slice(format!("xref\n0 {object_count}\n").as_bytes());
        buf.extend_from_slice(b"0000000000 65535 f \n");
        for &off in &offsets[1..] {
            buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(
            format!("trailer\n<< /Size {object_count} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF")
                .as_bytes(),
        );

        buf
    }

    fn escape_pdf_text(s: &str) -> String {
        s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::build_minimal_pdf;
    use super::*;

    #[test]
    fn extracts_text_from_a_single_page_pdf() {
        let bytes = build_minimal_pdf("Hello from a test PDF");
        let (title, sections) = parse(&bytes).unwrap();
        assert_eq!(title, None);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].page, Some(1));
        assert_eq!(sections[0].start_offset, 0);
        assert!(
            sections[0].text.contains("Hello from a test PDF"),
            "unexpected text: {:?}",
            sections[0].text
        );
    }

    #[test]
    fn invalid_pdf_bytes_produce_a_parse_error() {
        let result = parse(b"this is not a pdf");
        assert!(result.is_err());
    }
}
