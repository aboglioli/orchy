use std::sync::Arc;

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::{StatusCode, request::Parts};
use serde_json::json;

use orchy_core::organization::Organization;

use crate::container::Container;

pub struct OrgAuth(pub Organization);

impl FromRequestParts<Arc<Container>> for OrgAuth {
    type Rejection = (StatusCode, Json<serde_json::Value>);

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
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "missing or invalid Authorization header"})),
                )
            })?;

        let org = state
            .org_service
            .resolve_api_key(key)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": e.to_string()})),
                )
            })?
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": "invalid API key"})),
                )
            })?;

        Ok(OrgAuth(org))
    }
}
