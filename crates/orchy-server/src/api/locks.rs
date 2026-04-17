use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_core::agent::AgentId;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::ResourceLock;

use crate::container::Container;

use super::auth::OrgAuth;
use super::{ApiError, parse_namespace};

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_project(s: &str) -> Result<ProjectId, ApiError> {
    ProjectId::try_from(s.to_string())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_ns(ns: Option<&str>) -> Result<Namespace, ApiError> {
    match ns {
        Some(s) => parse_namespace(s),
        None => Ok(Namespace::root()),
    }
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), ApiError> {
    if auth.0.id() != org_id {
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
) -> Result<Json<Option<ResourceLock>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let lock = container
        .lock_service
        .check(&org_id, &project_id, &ns, &name)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(lock))
}

pub async fn acquire(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<AcquireBody>,
) -> Result<Json<ResourceLock>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(body.namespace.as_deref())?;

    let agent_id = body
        .agent_id
        .parse::<AgentId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let ttl = body.ttl_secs.unwrap_or(300);

    let lock = container
        .lock_service
        .acquire(org_id, project_id, ns, body.name, agent_id, ttl)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(lock))
}

pub async fn release(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, name)): Path<(String, String, String)>,
    Query(query): Query<ReleaseQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let agent_id = query
        .agent_id
        .parse::<AgentId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    container
        .lock_service
        .release(&org_id, &project_id, &ns, &name, &agent_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}
