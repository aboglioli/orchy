use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::user::TokenEncoder;

pub struct DecodeTokenCommand {
    pub token: String,
}

pub struct DecodeTokenResponse {
    pub user_id: String,
    pub email: String,
}

pub struct DecodeToken {
    token_encoder: Arc<dyn TokenEncoder>,
}

impl DecodeToken {
    pub fn new(token_encoder: Arc<dyn TokenEncoder>) -> Self {
        Self { token_encoder }
    }

    pub fn execute(&self, cmd: DecodeTokenCommand) -> Result<DecodeTokenResponse> {
        let claims = self.token_encoder.decode(&cmd.token)?;
        Ok(DecodeTokenResponse {
            user_id: claims.sub,
            email: claims.email,
        })
    }
}
