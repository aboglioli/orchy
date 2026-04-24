use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_application::{
    ChangeRolesCommand, CheckMailboxCommand, GetAgentCommand, GetAgentSummaryCommand,
    ListAgentsCommand, ListTasksCommand, RegisterAgentCommand, RenameAliasCommand,
    SwitchContextCommand, resolve_agent,
};
use orchy_core::agent::AgentId;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct ListAgentsQuery {
    pub project: Option<String>,
}

#[derive(Deserialize)]
pub struct AgentRefQuery {
    pub project: Option<String>,
}

#[derive(Serialize)]
pub struct AgentDto {
    pub id: String,
    pub alias: String,
    pub description: String,
    pub status: String,
    pub agent_type: Option<String>,
    pub project: Option<String>,
    pub namespace: String,
    pub roles: Vec<String>,
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

async fn resolve_agent_id(
    container: &Arc<Container>,
    org: &OrganizationId,
    id_or_alias: &str,
    project: Option<&str>,
) -> Result<String, ApiError> {
    if let Ok(agent_id) = id_or_alias.parse::<AgentId>() {
        let agent = container
            .agent_store
            .find_by_id(&agent_id)
            .await
            .map_err(ApiError::from)?
            .ok_or_else(|| {
                ApiError(
                    StatusCode::NOT_FOUND,
                    "NOT_FOUND",
                    "agent not found".to_string(),
                )
            })?;
        if agent.org_id() != org {
            return Err(ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "agent not found".to_string(),
            ));
        }
        return Ok(agent.id().to_string());
    }

    let project = project.ok_or_else(|| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            "project query param is required when addressing agent by alias".to_string(),
        )
    })?;
    let project = ProjectId::try_from(project.to_string())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let agent = resolve_agent(container.agent_store.as_ref(), org, &project, id_or_alias)
        .await
        .map_err(ApiError::from)?;
    Ok(agent.id().to_string())
}

pub async fn list(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Query(query): Query<ListAgentsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
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
        .filter(|a| {
            project_filter
                .as_ref()
                .map(|p| a.project == *p.as_ref())
                .unwrap_or(true)
        })
        .map(|a| AgentDto {
            id: a.id.clone(),
            alias: a.alias.clone(),
            description: a.description.clone(),
            status: a.status.clone(),
            agent_type: a.metadata.get("agent_type").cloned(),
            project: Some(a.project.clone()),
            namespace: a.namespace.clone(),
            roles: a.roles.clone(),
            last_seen: a.last_seen.clone(),
        })
        .collect();

    Ok(Json(serde_json::json!({"items": body})))
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
    Path(project): Path<String>,
    Json(body): Json<RegisterAgentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let cmd = RegisterAgentCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        alias: body.alias.clone(),
        roles: body.roles,
        description: body.description,
        agent_type: body.agent_type,
        metadata: body.metadata,
        auth_user_id: auth.user_id.clone(),
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
    Path(id): Path<String>,
    Query(query): Query<AgentRefQuery>,
) -> Result<Json<AgentContextDto>, ApiError> {
    let org = auth.org.id.clone();
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let agent_id = resolve_agent_id(&container, &org_id, &id, query.project.as_deref()).await?;

    let agent = container
        .app
        .get_agent
        .execute(GetAgentCommand {
            agent_id: agent_id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;

    if agent.org_id != org_id.to_string() {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    let mailbox_cmd = CheckMailboxCommand {
        agent_id,
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
        archived: None,
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
        alias: agent.alias.clone(),
        description: agent.description.clone(),
        status: agent.status.clone(),
        agent_type: agent.metadata.get("agent_type").cloned(),
        project: Some(agent.project.to_string()),
        namespace: agent.namespace.clone(),
        roles: agent.roles.clone(),
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
    Path(id): Path<String>,
    Query(query): Query<AgentRefQuery>,
    State(container): State<Arc<Container>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let agent_id = resolve_agent_id(&container, &org_id, &id, query.project.as_deref()).await?;

    let cmd = GetAgentSummaryCommand {
        org_id: org,
        agent_id,
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
    Path(id): Path<String>,
    Query(query): Query<AgentRefQuery>,
    Json(body): Json<ChangeRolesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let agent_id = resolve_agent_id(&container, &org_id, &id, query.project.as_deref()).await?;

    let cmd = ChangeRolesCommand {
        agent_id,
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

#[derive(Deserialize)]
pub struct RenameAliasBody {
    pub new_alias: String,
}

pub async fn rename_alias(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(id): Path<String>,
    Query(query): Query<AgentRefQuery>,
    Json(body): Json<RenameAliasBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let new_alias = body.new_alias.clone();
    let agent_id = resolve_agent_id(&container, &org_id, &id, query.project.as_deref()).await?;
    let cmd = RenameAliasCommand {
        agent_id,
        new_alias,
    };

    let agent = container
        .app
        .rename_alias
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

#[derive(Deserialize)]
pub struct SwitchContextBody {
    pub project: Option<String>,
    pub namespace: Option<String>,
}

pub async fn switch_context(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(id): Path<String>,
    Query(query): Query<AgentRefQuery>,
    Json(body): Json<SwitchContextBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let agent_id = resolve_agent_id(&container, &org_id, &id, query.project.as_deref()).await?;

    let cmd = SwitchContextCommand {
        org_id: org,
        agent_id,
        project: body.project,
        namespace: body.namespace,
    };

    let agent = container
        .app
        .switch_context
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
