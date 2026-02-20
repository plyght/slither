use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlitherError {
    #[error("crawl error: {0}")]
    Crawl(String),

    #[error("transform error: {0}")]
    Transform(String),

    #[error("index error: {0}")]
    Index(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type SlitherResult<T> = Result<T, SlitherError>;
