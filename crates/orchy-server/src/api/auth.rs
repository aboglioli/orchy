use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::{StatusCode, request::Parts};

use orchy_application::{ApiKeyPrincipal, ResolveApiKeyCommand, ResolveTokenCommand};

use crate::auth::cookie::AUTH_COOKIE_NAME;
use crate::container::Container;

use super::ApiError;

pub struct OrgAuth {
    pub org: orchy_application::OrganizationDto,
    pub user_id: Option<String>,
}

impl FromRequestParts<Arc<Container>> for OrgAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<Container>,
    ) -> Result<Self, Self::Rejection> {
        // 1. Try API key from Authorization header
        if let Some(token) = extract_bearer(parts) {
            if let Some(principal) = try_resolve_api_key(state, token).await {
                return Ok(OrgAuth {
                    org: principal.org,
                    user_id: principal.user_id,
                });
            }

            // 2. API key failed — try JWT fallback
            if let Some(auth) = try_resolve_jwt(state, token, parts).await {
                return Ok(auth);
            }
        }

        // 3. No Bearer header — try JWT from cookie
        if let Some(token) = extract_cookie(parts, AUTH_COOKIE_NAME) {
            if let Some(auth) = try_resolve_jwt(state, token, parts).await {
                return Ok(auth);
            }
        }

        Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "missing or invalid Authorization header or auth cookie".to_string(),
        ))
    }
}

fn extract_bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

fn extract_cookie<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts
        .headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(|c| c.trim())
                .find(|c| c.starts_with(&format!("{}=", name)))
                .and_then(|c| c.split_once('='))
                .map(|(_, v)| v)
        })
}

async fn try_resolve_api_key(state: &Arc<Container>, token: &str) -> Option<ApiKeyPrincipal> {
    state
        .app
        .resolve_api_key
        .execute(ResolveApiKeyCommand {
            key: token.to_string(),
        })
        .await
        .ok()
        .flatten()
}

async fn try_resolve_jwt(state: &Arc<Container>, token: &str, _parts: &Parts) -> Option<OrgAuth> {
    let resolve_token = state.app.resolve_token.as_ref()?;
    let principal = resolve_token
        .execute(ResolveTokenCommand {
            token: token.to_string(),
        })
        .await
        .ok()
        .flatten()?;

    Some(OrgAuth {
        org: principal.org,
        user_id: Some(principal.user_id),
    })
}
