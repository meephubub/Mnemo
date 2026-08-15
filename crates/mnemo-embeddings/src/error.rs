use thiserror::Error;

pub type Result<T> = std::result::Result<T, EmbedError>;

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("embedding model error: {0}")]
    Model(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}

impl From<EmbedError> for mnemo_core::MnemoError {
    fn from(err: EmbedError) -> Self {
        mnemo_core::MnemoError::Embedding(err.to_string())
    }
}