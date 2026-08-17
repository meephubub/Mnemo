//! `mnemo-ingest` — file identification, parsing, chunking, and
//! content hashing (plan.md section 14 "Document Ingestion" and
//! Phase 2 "Document Ingestion").
//!
//! This crate is pure/offline: it takes bytes in and produces
//! [`IngestedFile`] out, with no database or network access. The
//! top-level `mnemo` facade is responsible for turning that into
//! `Source` / `Document` / `Chunk` rows and persisting them.

pub mod chunker;
pub mod error;
pub mod hash;
pub mod parsers;

use std::path::Path;

pub use chunker::{ChunkConfig, ChunkDraft};
pub use error::{IngestError, Result};
pub use parsers::FileKind;

/// Bump when parsing/chunking logic changes in a way that should
/// trigger re-ingestion of previously indexed files (plan.md section
/// 15 "Incremental Indexing").
pub const PARSER_VERSION: &str = "0.1.0";

pub struct IngestedFile {
    pub kind: FileKind,
    pub mime_type: &'static str,
    pub title: Option<String>,
    pub content_hash: String,
    pub text: String,
    pub chunks: Vec<ChunkDraft>,
    pub parser_version: &'static str,
}

/// Read, parse, and chunk a file at `path` using the default chunking
/// configuration.
pub fn ingest_path(path: &Path) -> Result<IngestedFile> {
    ingest_path_with_config(path, &ChunkConfig::default())
}

pub fn ingest_path_with_config(path: &Path, config: &ChunkConfig) -> Result<IngestedFile> {
    let kind = parsers::identify(path)?;
    let raw = std::fs::read(path)?;
    ingest_bytes_with_config(kind, &raw, config)
}

/// Same as [`ingest_path_with_config`] but for in-memory content
/// (useful for tests, and for ingesting content that didn't come from
/// a file, e.g. a pasted note or an uploaded attachment).
///
/// Text-based formats (plain text, Markdown, HTML) must be valid
/// UTF-8; binary container formats (PDF, DOCX) are parsed directly
/// from bytes.
pub fn ingest_bytes_with_config(kind: FileKind, raw: &[u8], config: &ChunkConfig) -> Result<IngestedFile> {
    let parsed = parsers::parse(kind, raw)?;
    let chunks = chunker::chunk_parsed_file(&parsed, config);
    let content_hash = hash::content_hash(raw);

    Ok(IngestedFile {
        kind,
        mime_type: parsers::mime_type(kind),
        title: parsed.title,
        content_hash,
        text: parsed.text,
        chunks,
        parser_version: PARSER_VERSION,
    })
}

/// Convenience wrapper for the text-based formats (plain text,
/// Markdown, HTML): parse and chunk a UTF-8 string directly.
///
/// Returns [`IngestError::UnsupportedType`] for binary container
/// formats (PDF, DOCX) — those don't have a meaningful "raw string"
/// form; use [`ingest_bytes_with_config`] for them instead.
pub fn ingest_str_with_config(kind: FileKind, raw: &str, config: &ChunkConfig) -> Result<IngestedFile> {
    if parsers::is_binary(kind) {
        return Err(IngestError::UnsupportedType(format!(
            "{kind:?} is a binary format; use ingest_bytes_with_config instead of ingest_str_with_config"
        )));
    }
    ingest_bytes_with_config(kind, raw.as_bytes(), config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parsers::pdf::test_support::build_minimal_pdf;

    #[test]
    fn identify_recognizes_pdf_and_docx_extensions() {
        assert_eq!(parsers::identify(Path::new("report.pdf")).unwrap(), FileKind::Pdf);
        assert_eq!(parsers::identify(Path::new("report.PDF")).unwrap(), FileKind::Pdf);
        assert_eq!(parsers::identify(Path::new("memo.docx")).unwrap(), FileKind::Docx);
    }

    #[test]
    fn ingest_bytes_end_to_end_for_pdf_produces_paged_chunks() {
        let bytes = build_minimal_pdf("The quarterly report shows steady growth across all regions.");
        let ingested = ingest_bytes_with_config(FileKind::Pdf, &bytes, &ChunkConfig::default()).unwrap();

        assert_eq!(ingested.kind, FileKind::Pdf);
        assert_eq!(ingested.mime_type, "application/pdf");
        assert!(!ingested.chunks.is_empty());
        assert_eq!(ingested.chunks[0].page, Some(1));
        assert!(ingested.chunks[0].text.contains("quarterly report"));
        // content hash is deterministic and derived from the raw bytes
        assert_eq!(ingested.content_hash, hash::content_hash(&bytes));
    }

    #[test]
    fn ingest_bytes_end_to_end_for_docx_produces_headed_chunks() {
        use parsers::docx::test_support::{build_test_docx, DOC_NS};

        let xml = format!(
            r#"<w:document {DOC_NS}><w:body>
                <w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Quarterly Report</w:t></w:r></w:p>
                <w:p><w:r><w:t>Revenue grew steadily this quarter across all regions.</w:t></w:r></w:p>
            </w:body></w:document>"#
        );
        let bytes = build_test_docx(&xml, None);
        let ingested = ingest_bytes_with_config(FileKind::Docx, &bytes, &ChunkConfig::default()).unwrap();

        assert_eq!(ingested.kind, FileKind::Docx);
        assert_eq!(ingested.title.as_deref(), Some("Quarterly Report"));
        assert!(!ingested.chunks.is_empty());
        assert_eq!(ingested.chunks[0].section.as_deref(), Some("Quarterly Report"));
        assert!(ingested.chunks[0].text.contains("Revenue grew steadily"));
    }

    #[test]
    fn ingest_str_with_config_rejects_binary_kinds() {
        let result = ingest_str_with_config(FileKind::Pdf, "not real pdf bytes", &ChunkConfig::default());
        assert!(matches!(result, Err(IngestError::UnsupportedType(_))));
    }

    #[test]
    fn ingest_str_with_config_still_works_for_plain_text() {
        let ingested = ingest_str_with_config(FileKind::Text, "Hello, world.", &ChunkConfig::default()).unwrap();
        assert_eq!(ingested.chunks.len(), 1);
        assert_eq!(ingested.chunks[0].text, "Hello, world.");
    }
}
