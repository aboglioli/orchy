use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use orchy_core::error::{Error, Result};
use orchy_core::user::{Email, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,
    pub email: String,
    pub iat: i64,
    pub exp: i64,
}

pub struct JwtTokenEncoder {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    duration: Duration,
}

impl JwtTokenEncoder {
    pub fn from_rsa_pem(
        private_pem: &[u8],
        public_pem: &[u8],
        duration_hours: i64,
    ) -> Result<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(private_pem)
            .map_err(|e| Error::store(format!("invalid RSA private key: {e}")))?;
        let decoding_key = DecodingKey::from_rsa_pem(public_pem)
            .map_err(|e| Error::store(format!("invalid RSA public key: {e}")))?;

        Ok(Self {
            encoding_key,
            decoding_key,
            duration: Duration::hours(duration_hours),
        })
    }

    pub fn encode(&self, user_id: &UserId, email: &Email) -> Result<String> {
        let now = Utc::now();
        let claims = TokenClaims {
            sub: user_id.to_string(),
            email: email.as_str().to_string(),
            iat: now.timestamp(),
            exp: (now + self.duration).timestamp(),
        };

        jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &self.encoding_key)
            .map_err(|e| Error::store(format!("failed to encode JWT: {e}")))
    }

    pub fn decode(&self, token: &str) -> Result<TokenClaims> {
        let decoded = jsonwebtoken::decode::<TokenClaims>(
            token,
            &self.decoding_key,
            &Validation::new(Algorithm::RS256),
        )
        .map_err(|e| Error::authentication_failed(format!("invalid token: {e}")))?;

        Ok(decoded.claims)
    }
}

pub fn generate_rsa_keypair() -> Result<(String, String)> {
    use pkcs8::{EncodePrivateKey, EncodePublicKey};
    use rsa::RsaPrivateKey;

    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048)
        .map_err(|e| Error::store(format!("failed to generate RSA key: {e}")))?;

    let private_pem = private_key
        .to_pkcs8_pem(pkcs8::LineEnding::default())
        .map_err(|e| Error::store(format!("failed to encode private key: {e}")))?;

    let public_key = private_key.to_public_key();
    let public_pem = public_key
        .to_public_key_pem(pkcs8::LineEnding::default())
        .map_err(|e| Error::store(format!("failed to encode public key: {e}")))?;

    Ok((private_pem.to_string(), public_pem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_encode_decode() {
        let (private_pem, public_pem) = generate_rsa_keypair().unwrap();

        let encoder =
            JwtTokenEncoder::from_rsa_pem(private_pem.as_bytes(), public_pem.as_bytes(), 24)
                .unwrap();

        let user_id = UserId::new();
        let email = Email::new("test@example.com").unwrap();

        let token = encoder.encode(&user_id, &email).unwrap();
        let claims = encoder.decode(&token).unwrap();

        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.email, "test@example.com");
    }

    #[test]
    fn jwt_invalid_token() {
        let (private_pem, public_pem) = generate_rsa_keypair().unwrap();
        let encoder =
            JwtTokenEncoder::from_rsa_pem(private_pem.as_bytes(), public_pem.as_bytes(), 24)
                .unwrap();

        let result = encoder.decode("invalid.token.here");
        assert!(result.is_err());
    }
}
