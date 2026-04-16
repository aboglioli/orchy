use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_core::agent::AgentStatus;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct ListAgentsQuery {
    pub project: Option<String>,
}

#[derive(Serialize)]
pub struct AgentDto {
    pub id: String,
    pub alias: Option<String>,
    pub description: String,
    pub status: String,
    pub agent_type: Option<String>,
    pub namespace: String,
    pub last_heartbeat: String,
}

pub async fn list(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(org): Path<String>,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Json<Vec<AgentDto>>, (StatusCode, String)> {
    let org_id = OrganizationId::new(&org).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if auth.0.id() != &org_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".to_string()));
    }

    let agents = container
        .agent_service
        .list(&org_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let project_filter = query
        .project
        .as_deref()
        .map(|p| ProjectId::try_from(p.to_string()))
        .transpose()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let body: Vec<AgentDto> = agents
        .into_iter()
        .filter(|a| a.status() != AgentStatus::Disconnected)
        .filter(|a| {
            project_filter
                .as_ref()
                .map(|p| a.project() == p)
                .unwrap_or(true)
        })
        .map(|a| AgentDto {
            id: a.id().to_string(),
            alias: a.alias().map(|al| al.to_string()),
            description: a.description().to_string(),
            status: a.status().to_string(),
            agent_type: a.metadata().get("agent_type").cloned(),
            namespace: a.namespace().to_string(),
            last_heartbeat: a.last_heartbeat().to_rfc3339(),
        })
        .collect();

    Ok(Json(body))
}
