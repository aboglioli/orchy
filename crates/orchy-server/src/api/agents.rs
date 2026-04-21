use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_application::{
    ChangeRolesCommand, CheckMailboxCommand, GetAgentCommand, GetAgentSummaryCommand,
    ListAgentsCommand, ListTasksCommand, RegisterAgentCommand,
};
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
    pub last_seen: String,
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
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }

    let cmd = ListAgentsCommand {
        org_id: org,
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
        .filter(|a| a.status != "disconnected")
        .filter(|a| {
            project_filter
                .as_ref()
                .map(|p| a.project == p.as_ref())
                .unwrap_or(true)
        })
        .map(|a| AgentDto {
            id: a.id.clone(),
            description: a.description.clone(),
            status: a.status.clone(),
            agent_type: a.metadata.get("agent_type").cloned(),
            namespace: a.namespace.clone(),
            last_seen: a.last_seen.clone(),
        })
        .collect();

    Ok(Json(body))
}

#[derive(Deserialize)]
pub struct RegisterAgentBody {
    pub alias: String,
    pub description: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub agent_type: Option<String>,
    pub namespace: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

pub async fn register(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<RegisterAgentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }

    let cmd = RegisterAgentCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        alias: body.alias,
        roles: body.roles,
        description: body.description,
        agent_type: body.agent_type,
        metadata: body.metadata,
    };

    let agent = container
        .app
        .register_agent
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&agent).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn get_context(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
) -> Result<Json<AgentContextDto>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }

    let agent = container
        .app
        .get_agent
        .execute(GetAgentCommand {
            agent_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;

    if agent.org_id != org_id.to_string() || agent.status == "disconnected" {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    let mailbox_cmd = CheckMailboxCommand {
        agent_id: id.clone(),
        org_id: org.clone(),
        project: agent.project.clone(),
        after: None,
        limit: None,
    };

    let inbox = container
        .app
        .check_mailbox
        .execute(mailbox_cmd)
        .await
        .map_err(ApiError::from)?
        .items
        .into_iter()
        .map(|m| InboxMessageDto {
            id: m.id.clone(),
            from: m.from.clone(),
            body: m.body.clone(),
        })
        .collect();

    let tasks_cmd = ListTasksCommand {
        org_id: org,
        project: Some(agent.project.clone()),
        namespace: Some(agent.namespace.clone()),
        status: Some("pending".to_string()),
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
        .map_err(ApiError::from)?
        .items
        .into_iter()
        .map(|t| PendingTaskDto {
            id: t.id.clone(),
            title: t.title.clone(),
            priority: t.priority.clone(),
            assigned_roles: t.assigned_roles.clone(),
        })
        .collect();

    let agent_dto = AgentDto {
        id: agent.id.clone(),
        description: agent.description.clone(),
        status: agent.status.clone(),
        agent_type: agent.metadata.get("agent_type").cloned(),
        namespace: agent.namespace.clone(),
        last_seen: agent.last_seen.clone(),
    };

    Ok(Json(AgentContextDto {
        agent: agent_dto,
        inbox,
        pending_tasks,
    }))
}

pub async fn get_summary(
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    State(container): State<Arc<Container>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }

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

    let v = serde_json::to_value(&summary).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        )
    })?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct ChangeRolesBody {
    pub roles: Vec<String>,
}

pub async fn change_roles(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Json(body): Json<ChangeRolesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }

    let cmd = ChangeRolesCommand {
        agent_id: id,
        roles: body.roles,
    };

    let agent = container
        .app
        .change_roles
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let v = serde_json::to_value(&agent).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        )
    })?;
    Ok(Json(v))
}
