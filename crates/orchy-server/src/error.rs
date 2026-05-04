use thiserror::Error;

#[derive(Debug, Error)]
pub enum BootError {
    #[error("config error: {0}")]
    Config(String),

    #[error("store backend error: {0}")]
    Store(String),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("auth bootstrap error: {0}")]
    Auth(String),

    #[error("embeddings setup error: {0}")]
    Embeddings(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("other: {0}")]
    Other(String),
}

impl From<orchy_core::error::Error> for BootError {
    fn from(e: orchy_core::error::Error) -> Self {
        BootError::Store(e.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for BootError {
    fn from(e: Box<dyn std::error::Error>) -> Self {
        BootError::Other(e.to_string())
    }
}

pub type BootResult<T> = std::result::Result<T, BootError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_error_from_orchy_error_maps_to_store() {
        let core_err = orchy_core::error::Error::Store("oops".to_string());
        let boot_err = BootError::from(core_err);
        assert!(matches!(boot_err, BootError::Store(_)));
    }

    #[test]
    fn boot_error_displays_with_prefix() {
        let err = BootError::Store("db gone".to_string());
        assert_eq!(err.to_string(), "store backend error: db gone");

        let err = BootError::Config("missing field".to_string());
        assert_eq!(err.to_string(), "config error: missing field");

        let err = BootError::Migration("schema drift".to_string());
        assert_eq!(err.to_string(), "migration error: schema drift");

        let err = BootError::Auth("no keys".to_string());
        assert_eq!(err.to_string(), "auth bootstrap error: no keys");

        let err = BootError::Embeddings("bad provider".to_string());
        assert_eq!(err.to_string(), "embeddings setup error: bad provider");
    }
}
