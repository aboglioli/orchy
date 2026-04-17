use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::{StatusCode, request::Parts};

use orchy_core::organization::Organization;

use crate::container::Container;

use super::ApiError;

pub struct OrgAuth(pub Organization);

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

        let org = state
            .org_service
            .resolve_api_key(key)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| {
                ApiError(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "invalid or expired API key".to_string(),
                )
            })?;

        Ok(OrgAuth(org))
    }
}
