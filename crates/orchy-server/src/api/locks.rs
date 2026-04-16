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

fn parse_org(s: &str) -> Result<OrganizationId, (StatusCode, String)> {
    OrganizationId::new(s).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

fn parse_project(s: &str) -> Result<ProjectId, (StatusCode, String)> {
    ProjectId::try_from(s.to_string()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

fn parse_ns(ns: Option<&str>) -> Result<Namespace, (StatusCode, String)> {
    match ns {
        Some(s) => Namespace::try_from(format!("/{s}"))
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string())),
        None => Ok(Namespace::root()),
    }
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), (StatusCode, String)> {
    if auth.0.id() != org_id {
        Err((StatusCode::FORBIDDEN, "forbidden".to_string()))
    } else {
        Ok(())
    }
}

fn map_err(e: orchy_core::error::Error) -> (StatusCode, String) {
    use orchy_core::error::Error;
    match &e {
        Error::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        Error::Conflict(_) => (StatusCode::CONFLICT, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
pub struct NsQuery {
    pub ns: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseQuery {
    pub agent_id: String,
    pub ns: Option<String>,
}

#[derive(Deserialize)]
pub struct AcquireBody {
    pub name: String,
    pub ns: Option<String>,
    pub ttl_secs: Option<u64>,
    pub agent_id: String,
}

pub async fn check(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, name)): Path<(String, String, String)>,
    Query(query): Query<NsQuery>,
) -> Result<Json<Option<ResourceLock>>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.ns.as_deref())?;

    let lock = container
        .lock_service
        .check(&org_id, &project_id, &ns, &name)
        .await
        .map_err(map_err)?;

    Ok(Json(lock))
}

pub async fn acquire(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<AcquireBody>,
) -> Result<Json<ResourceLock>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(body.ns.as_deref())?;

    let agent_id = body
        .agent_id
        .parse::<AgentId>()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let ttl = body.ttl_secs.unwrap_or(300);

    let lock = container
        .lock_service
        .acquire(org_id, project_id, ns, body.name, agent_id, ttl)
        .await
        .map_err(map_err)?;

    Ok(Json(lock))
}

pub async fn release(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, name)): Path<(String, String, String)>,
    Query(query): Query<ReleaseQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.ns.as_deref())?;

    let agent_id = query
        .agent_id
        .parse::<AgentId>()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    container
        .lock_service
        .release(&org_id, &project_id, &ns, &name, &agent_id)
        .await
        .map_err(map_err)?;

    Ok(Json(serde_json::json!({"ok": true})))
}
