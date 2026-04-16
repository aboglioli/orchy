use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use orchy_core::agent::AgentStatus;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

#[derive(Deserialize)]
pub struct ListAgentsQuery {
    pub project: String,
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

pub async fn list_agents(
    State(container): State<Arc<Container>>,
    Query(params): Query<ListAgentsQuery>,
) -> impl IntoResponse {
    let org = OrganizationId::new("default").unwrap();

    let project_id = match orchy_core::namespace::ProjectId::try_from(params.project) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    match container.agent_service.list(&org).await {
        Ok(agents) => {
            let body: Vec<AgentDto> = agents
                .into_iter()
                .filter(|a| *a.project() == project_id && a.status() != AgentStatus::Disconnected)
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
            Json(body).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
