use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_application::{CheckLockCommand, LockResourceCommand, UnlockResourceCommand};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

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
pub struct NamespaceQuery {
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseQuery {
    pub agent_id: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct AcquireBody {
    pub name: String,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
    pub ttl_secs: Option<u64>,
    pub agent_id: String,
}

pub async fn check(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, name)): Path<(String, String, String)>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = CheckLockCommand {
        org_id: org,
        project,
        namespace: query.namespace,
        name,
    };

    let lock = container
        .app
        .check_lock
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&lock).unwrap_or_default()))
}

pub async fn acquire(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<AcquireBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = LockResourceCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        name: body.name,
        holder_agent_id: body.agent_id,
        ttl_secs: body.ttl_secs,
    };

    let lock = container
        .app
        .lock_resource
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&lock).unwrap_or_default()))
}

pub async fn release(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, name)): Path<(String, String, String)>,
    Query(query): Query<ReleaseQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = UnlockResourceCommand {
        org_id: org,
        project,
        namespace: query.namespace,
        name,
        holder_agent_id: query.agent_id,
    };

    container
        .app
        .unlock_resource
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}
