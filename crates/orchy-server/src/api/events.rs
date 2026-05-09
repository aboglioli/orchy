use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use orchy_events::SerializedEvent;
use serde::Deserialize;

use orchy_application::PollUpdatesCommand;
use orchy_core::namespace::Namespace;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct PollQuery {
    pub since: Option<String>,
    pub limit: Option<u32>,
    pub namespace: Option<String>,
}

#[tracing::instrument(skip_all)]
pub async fn poll(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(_project): Path<String>,
    Query(query): Query<PollQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let since_str = match query.since.as_deref() {
        Some(s) => {
            DateTime::parse_from_rfc3339(s).map_err(|e| {
                ApiError(
                    StatusCode::BAD_REQUEST,
                    "INVALID_PARAM",
                    format!("invalid timestamp: {e}"),
                )
            })?;
            s.to_owned()
        }
        None => (Utc::now() - Duration::minutes(5)).to_rfc3339(),
    };

    let since_parsed = since_str
        .parse::<DateTime<Utc>>()
        .unwrap_or_else(|_| Utc::now() - Duration::minutes(5));

    let namespace_prefix = if let Some(ref ns) = query.namespace {
        let namespace = Namespace::new(ns).map_err(|e| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "INVALID_PARAM",
                format!("invalid namespace: {e}"),
            )
        })?;
        Some(namespace.to_string())
    } else {
        None
    };

    let cmd = PollUpdatesCommand {
        organization: org.to_string(),
        since: since_str,
        limit: query.limit,
        topics: None,
        namespace_prefix,
    };

    let events = container
        .app
        .poll_updates
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let updates: Vec<_> = events
        .iter()
        .filter_map(|e| SerializedEvent::from_event(e).ok())
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
