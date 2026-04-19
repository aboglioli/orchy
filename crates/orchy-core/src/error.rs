use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u64, actual: u64 },

    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("dependency not met: task {0} is not completed")]
    DependencyNotMet(String),

    #[error("store error: {0}")]
    Store(String),

    #[error("embeddings error: {0}")]
    Embeddings(String),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn store(msg: impl Into<String>) -> Self {
        Self::Store(msg.into())
    }

    pub fn authentication_failed(msg: impl Into<String>) -> Self {
        Self::AuthenticationFailed(msg.into())
    }
}
