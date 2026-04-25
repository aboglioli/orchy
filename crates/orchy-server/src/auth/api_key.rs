use orchy_core::api_key::{
    ApiKey, ApiKeyGenerator, ApiKeyPrefix, ApiKeySuffix, HashedApiKey, PlainApiKey,
};
use orchy_core::error::Result;
use orchy_core::organization::OrganizationId;
use orchy_core::user::UserId;

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
    fn generate(
        &self,
        org_id: &OrganizationId,
        user_id: Option<UserId>,
        name: String,
    ) -> Result<(PlainApiKey, ApiKey)> {
        use rand::Rng;
        let mut buf = [0u8; SECRET_BYTES];
        rand::thread_rng().fill(&mut buf);
        let raw = format!("{PREFIX}{}", hex::encode(buf));
        let plain = PlainApiKey::new(raw)?;

        let hashed = self.hash(&plain);
        let prefix = ApiKeyPrefix::new(plain.prefix().to_string())?;
        let suffix = ApiKeySuffix::new(plain.suffix().to_string())?;

        let api_key = ApiKey::new(org_id.clone(), name, hashed, prefix, suffix, user_id);
        Ok((plain, api_key))
    }

    fn hash(&self, plain: &PlainApiKey) -> HashedApiKey {
        use sha2::Digest;
        let hash = format!("{:x}", sha2::Sha256::digest(plain.as_str().as_bytes()));
        HashedApiKey::new(hash).expect("SHA256 hash is never empty")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_creates_valid_key_pair() {
        let generator = RandomApiKeyGenerator::new();
        let org_id = OrganizationId::new("test").unwrap();
        let (plain, api_key) = generator
            .generate(&org_id, None, "test-key".into())
            .unwrap();

        assert!(plain.as_str().starts_with("sk_"));
        assert_eq!(plain.as_str().len(), 67);
        assert_eq!(api_key.name(), "test-key");
        assert_eq!(api_key.org_id(), &org_id);
        assert!(api_key.is_active());
    }

    #[test]
    fn hash_is_consistent() {
        let generator = RandomApiKeyGenerator::new();
        let plain = PlainApiKey::new(
            "sk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        let h1 = generator.hash(&plain);
        let h2 = generator.hash(&plain);
        assert_eq!(h1.as_str(), h2.as_str());
        assert_eq!(h1.as_str().len(), 64);
    }

    #[test]
    fn different_keys_produce_different_hashes() {
        let generator = RandomApiKeyGenerator::new();
        let org_id = OrganizationId::new("test").unwrap();
        let (p1, _) = generator.generate(&org_id, None, "k1".into()).unwrap();
        let (p2, _) = generator.generate(&org_id, None, "k2".into()).unwrap();
        assert_ne!(generator.hash(&p1).as_str(), generator.hash(&p2).as_str());
    }
}
