use thiserror::Error;

pub type Result<T> = std::result::Result<T, SearchError>;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("storage error: {0}")]
    Storage(#[from] mnemo_storage::StorageError),

    #[error("invalid query: {0}")]
    InvalidQuery(String),
}

impl From<SearchError> for mnemo_core::MnemoError {
    fn from(err: SearchError) -> Self {
        mnemo_core::MnemoError::Search(err.to_string())
    }
}
