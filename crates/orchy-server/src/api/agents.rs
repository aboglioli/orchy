use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_application::{
    CheckMailboxCommand, GetAgentSummaryCommand, ListAgentsCommand, ListTasksCommand,
};
use orchy_core::agent::AgentStatus;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct ListAgentsQuery {
    pub project: Option<String>,
}

#[derive(Serialize)]
pub struct AgentDto {
    pub id: String,
    pub description: String,
    pub status: String,
    pub agent_type: Option<String>,
    pub namespace: String,
    pub last_heartbeat: String,
}

#[derive(Serialize)]
pub struct AgentContextDto {
    pub agent: AgentDto,
    pub inbox: Vec<InboxMessageDto>,
    pub pending_tasks: Vec<PendingTaskDto>,
}

#[derive(Serialize)]
pub struct InboxMessageDto {
    pub id: String,
    pub from: String,
    pub body: String,
}

#[derive(Serialize)]
pub struct PendingTaskDto {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub assigned_roles: Vec<String>,
}

pub async fn list(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(org): Path<String>,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Json<Vec<AgentDto>>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id() != &org_id {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "forbidden".to_string(),
        ));
    }

    let cmd = ListAgentsCommand {
        org_id: Some(org),
        project: None,
        after: None,
        limit: None,
    };

    let agents = container
        .app
        .list_agents
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let project_filter = query
        .project
        .as_deref()
        .map(|p| ProjectId::try_from(p.to_string()))
        .transpose()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let body: Vec<AgentDto> = agents
        .items
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
            description: a.description().to_string(),
            status: a.status().to_string(),
            agent_type: a.metadata().get("agent_type").cloned(),
            namespace: a.namespace().to_string(),
            last_heartbeat: a.last_heartbeat().to_rfc3339(),
        })
        .collect();

    Ok(Json(body))
}

pub async fn get_context(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
) -> Result<Json<AgentContextDto>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id() != &org_id {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "forbidden".to_string(),
        ));
    }

    let agent = container
        .app
        .get_agent
        .execute(&id)
        .await
        .map_err(ApiError::from)?;

    if agent.org_id() != &org_id || agent.status() == AgentStatus::Disconnected {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    let mailbox_cmd = CheckMailboxCommand {
        agent_id: id.clone(),
        org_id: org.clone(),
        project: agent.project().to_string(),
        namespace: Some(agent.namespace().to_string()),
        after: None,
        limit: None,
    };

    let inbox = container
        .app
        .check_mailbox
        .execute(mailbox_cmd)
        .await
        .map(|p| p.items)
        .unwrap_or_default()
        .into_iter()
        .map(|m| InboxMessageDto {
            id: m.id().to_string(),
            from: m.from().to_string(),
            body: m.body().to_string(),
        })
        .collect();

    let tasks_cmd = ListTasksCommand {
        org_id: Some(org),
        project: Some(agent.project().to_string()),
        namespace: Some(agent.namespace().to_string()),
        status: Some("pending".to_string()),
        parent_id: None,
        assigned_to: None,
        tag: None,
        after: None,
        limit: Some(10),
    };

    let pending_tasks = container
        .app
        .list_tasks
        .execute(tasks_cmd)
        .await
        .map(|p| p.items)
        .unwrap_or_default()
        .into_iter()
        .map(|t| PendingTaskDto {
            id: t.id().to_string(),
            title: t.title().to_string(),
            priority: t.priority().to_string(),
            assigned_roles: t.assigned_roles().to_vec(),
        })
        .collect();

    let agent_dto = AgentDto {
        id: agent.id().to_string(),
        description: agent.description().to_string(),
        status: agent.status().to_string(),
        agent_type: agent.metadata().get("agent_type").cloned(),
        namespace: agent.namespace().to_string(),
        last_heartbeat: agent.last_heartbeat().to_rfc3339(),
    };

    Ok(Json(AgentContextDto {
        agent: agent_dto,
        inbox,
        pending_tasks,
    }))
}

pub async fn get_summary(
    _auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    State(container): State<Arc<Container>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cmd = GetAgentSummaryCommand {
        org_id: org,
        agent_id: id,
    };

    let summary = container
        .app
        .get_agent_summary
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&summary).unwrap_or_default()))
}
