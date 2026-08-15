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
    let raw = std::fs::read_to_string(path)?;
    ingest_str_with_config(kind, &raw, config)
}

/// Same as [`ingest_path_with_config`] but for in-memory content
/// (useful for tests, and for ingesting content that didn't come from
/// a file, e.g. a pasted note).
pub fn ingest_str_with_config(kind: FileKind, raw: &str, config: &ChunkConfig) -> Result<IngestedFile> {
    let parsed = parsers::parse(kind, raw)?;
    let chunks = chunker::chunk_parsed_file(&parsed, config);
    let content_hash = hash::content_hash(raw.as_bytes());

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
