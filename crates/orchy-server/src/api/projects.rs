use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_core::namespace::{NamespaceStore, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct UpdateProjectBody {
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct SetMetadataBody {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct IncludeSummaryQuery {
    pub include_summary: Option<bool>,
}

#[derive(Serialize)]
pub struct ProjectDto {
    pub id: String,
    pub description: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

fn project_to_dto(p: orchy_core::project::Project) -> ProjectDto {
    ProjectDto {
        id: p.id().to_string(),
        description: p.description().to_string(),
        metadata: p.metadata().clone(),
        created_at: p.created_at().to_rfc3339(),
        updated_at: p.updated_at().to_rfc3339(),
    }
}

fn parse_org_id(org: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_project_id(project: &str) -> Result<ProjectId, ApiError> {
    ProjectId::try_from(project.to_string())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
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

pub async fn get(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Query(query): Query<IncludeSummaryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org_id(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project_id(&project)?;

    let project = container
        .project_service
        .get_or_create(&org_id, &project_id)
        .await
        .map_err(|e| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                e.to_string(),
            )
        })?;

    if !query.include_summary.unwrap_or(false) {
        return Ok(Json(serde_json::to_value(project_to_dto(project)).unwrap()));
    }

    let agents = container
        .agent_service
        .list(&org_id)
        .await
        .unwrap_or_default();
    let project_agents: Vec<_> = agents
        .into_iter()
        .filter(|a| *a.project() == project_id)
        .collect();

    let tasks = container
        .task_service
        .list(orchy_core::task::TaskFilter {
            project: Some(project_id),
            ..Default::default()
        })
        .await
        .unwrap_or_default();

    let mut by_status = std::collections::HashMap::new();
    for task in &tasks {
        *by_status.entry(task.status().to_string()).or_insert(0u32) += 1;
    }

    Ok(Json(serde_json::json!({
        "project": project_to_dto(project),
        "summary": {
            "agents_count": project_agents.len(),
            "tasks_by_status": by_status,
            "total_tasks": tasks.len(),
        }
    })))
}

pub async fn update(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<UpdateProjectBody>,
) -> Result<Json<ProjectDto>, ApiError> {
    let org_id = parse_org_id(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project_id(&project)?;

    let description = body.description.unwrap_or_default();
    let project = container
        .project_service
        .update_description(&org_id, &project_id, description)
        .await
        .map_err(|e| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                e.to_string(),
            )
        })?;

    Ok(Json(project_to_dto(project)))
}

pub async fn set_metadata(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<SetMetadataBody>,
) -> Result<Json<ProjectDto>, ApiError> {
    let org_id = parse_org_id(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project_id(&project)?;

    let project = container
        .project_service
        .set_metadata(&org_id, &project_id, body.key, body.value)
        .await
        .map_err(|e| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                e.to_string(),
            )
        })?;

    Ok(Json(project_to_dto(project)))
}

pub async fn list_namespaces(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, ApiError> {
    let org_id = parse_org_id(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project_id(&project)?;

    let namespaces = NamespaceStore::list(&*container.store, &org_id, &project_id)
        .await
        .map_err(|e| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                e.to_string(),
            )
        })?;

    Ok(Json(namespaces.iter().map(|n| n.to_string()).collect()))
}
