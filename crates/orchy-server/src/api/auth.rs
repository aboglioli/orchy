use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::{StatusCode, request::Parts};

use orchy_application::ResolveApiKeyCommand;

use crate::container::Container;

use super::ApiError;

pub struct OrgAuth {
    pub org: orchy_application::OrganizationResponse,
    pub user_id: Option<String>,
}

impl FromRequestParts<Arc<Container>> for OrgAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                ApiError(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "missing or invalid Authorization header".to_string(),
                )
            })?;

        let principal = state
            .app
            .resolve_api_key
            .execute(ResolveApiKeyCommand {
                key: key.to_string(),
            })
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| {
                ApiError(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "invalid or expired API key".to_string(),
                )
            })?;

        Ok(OrgAuth {
            org: principal.org,
            user_id: principal.user_id,
        })
    }
}
