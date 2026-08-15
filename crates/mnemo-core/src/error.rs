//! Shared error type for the Mnemo workspace.

use thiserror::Error;

/// A convenience `Result` alias used throughout Mnemo crates.
pub type Result<T> = std::result::Result<T, MnemoError>;

/// Top-level error type returned by Mnemo APIs.
///
/// Individual crates (storage, ingest, search, ...) define their own
/// error enums and convert into this one at their public boundary so
/// callers of the top-level `mnemo` facade only ever need to match on
/// a single error type.
#[derive(Debug, Error)]
pub enum MnemoError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("ingestion error: {0}")]
    Ingest(String),

    #[error("search error: {0}")]
    Search(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl MnemoError {
    pub fn other(msg: impl Into<String>) -> Self {
        MnemoError::Other(msg.into())
    }
}
