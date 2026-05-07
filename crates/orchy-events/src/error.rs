use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid topic: {0}")]
    InvalidTopic(String),

    #[error("invalid namespace: {0}")]
    InvalidNamespace(String),

    #[error("invalid organization: {0}")]
    InvalidOrganization(String),

    #[error("invalid metadata key: {0}")]
    InvalidMetadataKey(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),

    #[error("invalid consumer group id: {0}")]
    InvalidConsumerGroupId(String),

    #[error("invalid start position: {0}")]
    InvalidStartFrom(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("store error: {0}")]
    Store(String),

    #[error("timeout: {0}")]
    Timeout(String),
}

pub type Result<T> = std::result::Result<T, Error>;
