use thiserror::Error;

#[derive(Error, Debug)]
pub enum BrainError {
    #[error("conflict: {0}")]
    Conflict(String),

    #[error("missing embedding: {chunk_id}")]
    MissingEmbedding { chunk_id: i64 },

    #[error("dictionary not found: {lang}")]
    DictionaryNotFound { lang: String },

    #[error("API unreachable: {provider} - {message}")]
    ApiUnreachable { provider: String, message: String },

    #[error("model dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(#[from] figment::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BrainError>;
