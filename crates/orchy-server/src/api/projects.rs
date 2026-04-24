use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use crate::container::Container;
use orchy_application::{
    GetProjectCommand, GetProjectOverviewCommand, ListNamespacesCommand, SetProjectMetadataCommand,
    UpdateProjectCommand,
};

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
    pub namespace: Option<String>,
}

#[derive(Serialize)]
pub struct ProjectDto {
    pub id: String,
    pub description: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

fn project_to_dto(p: orchy_application::ProjectResponse) -> ProjectDto {
    ProjectDto {
        id: p.id,
        description: p.description,
        metadata: p.metadata,
        created_at: p.created_at,
        updated_at: p.updated_at,
    }
}

pub async fn get(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(project): Path<String>,
    Query(query): Query<IncludeSummaryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    if !query.include_summary.unwrap_or(false) {
        let cmd = GetProjectCommand {
            org_id: org,
            project,
        };

        let p = container
            .app
            .get_project
            .execute(cmd)
            .await
            .map_err(ApiError::from)?;

        let v = serde_json::to_value(project_to_dto(p)).map_err(|e| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                e.to_string(),
            )
        })?;
        return Ok(Json(v));
    }

    let cmd = GetProjectOverviewCommand {
        org_id: org,
        project,
        namespace: query.namespace,
    };

    let overview = container
        .app
        .get_project_overview
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let mut by_status = std::collections::HashMap::new();
    for task in &overview.tasks {
        *by_status.entry(task.status.clone()).or_insert(0u32) += 1;
    }

    let project_dto = overview
        .project
        .map(project_to_dto)
        .map(|p| {
            serde_json::to_value(p).map_err(|e| {
                ApiError(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SERIALIZATION_ERROR",
                    e.to_string(),
                )
            })
        })
        .transpose()?;

    Ok(Json(serde_json::json!({
        "project": project_dto,
        "summary": {
            "agents_count": overview.agents.len(),
            "tasks_by_status": by_status,
            "total_tasks": overview.tasks.len(),
        }
    })))
}

pub async fn update(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(project): Path<String>,
    Json(body): Json<UpdateProjectBody>,
) -> Result<Json<ProjectDto>, ApiError> {
    let org = auth.org.id.clone();
    let cmd = UpdateProjectCommand {
        org_id: org,
        project,
        description: body.description.unwrap_or_default(),
    };

    let p = container
        .app
        .update_project
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(project_to_dto(p)))
}

pub async fn set_metadata(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(project): Path<String>,
    Json(body): Json<SetMetadataBody>,
) -> Result<Json<ProjectDto>, ApiError> {
    let org = auth.org.id.clone();
    let cmd = SetProjectMetadataCommand {
        org_id: org,
        project,
        key: body.key,
        value: body.value,
    };

    let p = container
        .app
        .set_project_metadata
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(project_to_dto(p)))
}

pub async fn list_namespaces(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(project): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let org = auth.org.id.clone();
    let cmd = ListNamespacesCommand {
        org_id: org,
        project,
    };

    let namespaces = container
        .app
        .list_namespaces
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(namespaces.iter().map(|n| n.to_string()).collect()))
}
