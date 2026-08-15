use thiserror::Error;

pub type Result<T> = std::result::Result<T, IngestError>;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("unsupported file type: {0}")]
    UnsupportedType(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(String),
}

impl From<IngestError> for mnemo_core::MnemoError {
    fn from(err: IngestError) -> Self {
        mnemo_core::MnemoError::Ingest(err.to_string())
    }
}
