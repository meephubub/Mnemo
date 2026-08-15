use thiserror::Error;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database migration failed: {0}")]
    Migration(String),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("row decode error: {0}")]
    Decode(String),

    #[error("not found: {0}")]
    NotFound(String),
}

impl From<StorageError> for mnemo_core::MnemoError {
    fn from(err: StorageError) -> Self {
        mnemo_core::MnemoError::Storage(err.to_string())
    }
}
