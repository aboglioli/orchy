use crate::error::Result;
use crate::organization::{ApiKeyHash, ApiKeyPrefix, RawApiKey};

pub trait ApiKeyGenerator: Send + Sync {
    fn generate(&self) -> RawApiKey;
    fn hash(&self, raw_key: &RawApiKey) -> Result<ApiKeyHash>;
    fn extract_prefix(&self, raw_key: &RawApiKey) -> Result<ApiKeyPrefix>;
}
