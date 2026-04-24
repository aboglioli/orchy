use orchy_core::api_key::ApiKeyGenerator;
use orchy_core::error::Result;
use orchy_core::organization::{ApiKeyHash, ApiKeyPrefix, RawApiKey};

const PREFIX: &str = "sk_";
const SECRET_BYTES: usize = 32; // 256 bits of entropy

pub struct RandomApiKeyGenerator;

impl RandomApiKeyGenerator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RandomApiKeyGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiKeyGenerator for RandomApiKeyGenerator {
    fn generate(&self) -> RawApiKey {
        use rand::Rng;
        let mut buf = [0u8; SECRET_BYTES];
        rand::thread_rng().fill(&mut buf);
        let raw = format!("{PREFIX}{}", hex::encode(buf));
        RawApiKey::new(raw).expect("generated key should always be valid")
    }

    fn hash(&self, raw_key: &RawApiKey) -> Result<ApiKeyHash> {
        use sha2::Digest;
        let hash = format!("{:x}", sha2::Sha256::digest(raw_key.as_str().as_bytes()));
        ApiKeyHash::new(hash)
    }

    fn extract_prefix(&self, raw_key: &RawApiKey) -> Result<ApiKeyPrefix> {
        ApiKeyPrefix::new(raw_key.as_str()[..8].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_returns_correct_prefix() {
        let generator = RandomApiKeyGenerator::new();
        let key = generator.generate();
        assert!(key.as_str().starts_with("sk_"));
        assert_eq!(key.as_str().len(), PREFIX.len() + SECRET_BYTES * 2);
    }

    #[test]
    fn extract_prefix_returns_first_8_chars() {
        let generator = RandomApiKeyGenerator::new();
        let key = generator.generate();
        let prefix = generator.extract_prefix(&key).unwrap();
        assert_eq!(prefix.as_str().len(), 8);
        assert_eq!(prefix.as_str(), &key.as_str()[..8]);
    }

    #[test]
    fn hash_is_consistent() {
        let generator = RandomApiKeyGenerator::new();
        let raw = RawApiKey::new(
            "sk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        let h1 = generator.hash(&raw).unwrap();
        let h2 = generator.hash(&raw).unwrap();
        assert_eq!(h1.as_str(), h2.as_str());
        assert_eq!(h1.as_str().len(), 64);
    }

    #[test]
    fn different_keys_produce_different_hashes() {
        let generator = RandomApiKeyGenerator::new();
        let k1 = generator.generate();
        let k2 = generator.generate();
        assert_ne!(k1.as_str(), k2.as_str());
        assert_ne!(
            generator.hash(&k1).unwrap().as_str(),
            generator.hash(&k2).unwrap().as_str()
        );
    }
}
