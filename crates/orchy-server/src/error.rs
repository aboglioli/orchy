use orchy_core::error::Error as CoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BootError {
    #[error("config error: {0}")]
    Config(String),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("auth bootstrap error: {0}")]
    Auth(String),

    #[error("embeddings setup error: {0}")]
    EmbeddingsProvider(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Domain(#[from] CoreError),

    #[error("other: {0}")]
    Other(String),
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
    use orchy_core::error::StoreError;

    #[test]
    fn boot_error_from_core_error_preserves_variant() {
        let core_err = CoreError::Store(StoreError::Other("oops".to_string()));
        let boot_err = BootError::from(core_err);
        assert!(matches!(
            boot_err,
            BootError::Domain(CoreError::Store(StoreError::Other(_)))
        ));
    }

    #[test]
    fn boot_error_displays_with_prefix() {
        let err = BootError::Config("missing field".to_string());
        assert_eq!(err.to_string(), "config error: missing field");

        let err = BootError::Migration("schema drift".to_string());
        assert_eq!(err.to_string(), "migration error: schema drift");

        let err = BootError::Auth("no keys".to_string());
        assert_eq!(err.to_string(), "auth bootstrap error: no keys");

        let err = BootError::EmbeddingsProvider("bad provider".to_string());
        assert_eq!(err.to_string(), "embeddings setup error: bad provider");
    }
}
