use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_application::PollUpdatesCommand;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::auth::OrgAuth;
use super::{ApiError, parse_namespace};

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), ApiError> {
    if auth.0.id.as_str() != org_id.as_str() {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "forbidden".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct PollQuery {
    pub since: Option<String>,
    pub limit: Option<u32>,
    pub namespace: Option<String>,
}

pub async fn poll(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project)): Path<(String, String)>,
    Query(query): Query<PollQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let since_str = match query.since.as_deref() {
        Some(s) => {
            chrono::DateTime::parse_from_rfc3339(s).map_err(|e| {
                ApiError(
                    StatusCode::BAD_REQUEST,
                    "INVALID_PARAM",
                    format!("invalid timestamp: {e}"),
                )
            })?;
            s.to_string()
        }
        None => (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339(),
    };

    let since_parsed = since_str
        .parse::<chrono::DateTime<chrono::Utc>>()
        .unwrap_or_else(|_| chrono::Utc::now() - chrono::Duration::minutes(5));

    let cmd = PollUpdatesCommand {
        org_id: org,
        since: since_str,
        limit: query.limit,
    };

    let mut events = container
        .app
        .poll_updates
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    if let Some(ref ns) = query.namespace {
        let namespace = parse_namespace(ns)?;
        let ns_str = namespace.to_string();
        events.retain(|e| e.namespace == ns_str || e.namespace.starts_with(&format!("{ns_str}/")));
    }

    let updates: Vec<_> = events
        .iter()
        .map(|e| {
            serde_json::json!({
                "topic": e.topic,
                "namespace": e.namespace,
                "payload": e.payload,
                "timestamp": e.timestamp.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "since": since_parsed.to_rfc3339(),
        "count": updates.len(),
        "events": updates,
    })))
}
