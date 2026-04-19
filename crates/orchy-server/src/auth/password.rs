use orchy_core::error::{Error, Result};
use orchy_core::user::{HashedPassword, PasswordHasher, PlainPassword};

pub struct BcryptPasswordHasher {
    cost: u32,
}

impl BcryptPasswordHasher {
    pub fn new() -> Self {
        Self {
            cost: bcrypt::DEFAULT_COST,
        }
    }

    pub fn with_cost(cost: u32) -> Self {
        Self { cost }
    }
}

impl Default for BcryptPasswordHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl PasswordHasher for BcryptPasswordHasher {
    fn hash(&self, plain: &PlainPassword) -> Result<HashedPassword> {
        let hash = bcrypt::hash(plain.as_str(), self.cost)
            .map_err(|e| Error::store(format!("failed to hash password: {e}")))?;
        HashedPassword::new(&hash)
    }

    fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> Result<()> {
        let valid = bcrypt::verify(plain.as_str(), hashed.as_str())
            .map_err(|e| Error::store(format!("bcrypt error: {e}")))?;
        if valid {
            Ok(())
        } else {
            Err(Error::authentication_failed("invalid credentials"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcrypt_hash_verify() {
        let hasher = BcryptPasswordHasher::new();
        let password = PlainPassword::new("password123").unwrap();

        let hashed = hasher.hash(&password).unwrap();
        assert!(hasher.verify(&password, &hashed).is_ok());

        let wrong_password = PlainPassword::new("wrongpass").unwrap();
        assert!(hasher.verify(&wrong_password, &hashed).is_err());
    }
}
