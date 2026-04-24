use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_application::{
    CheckMailboxCommand, CheckSentMessagesCommand, ListConversationCommand, MarkReadCommand,
    SendMessageCommand, resolve_agent,
};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

use orchy_core::agent::AgentId;
use orchy_core::message::MessageId;
use std::str::FromStr;

pub async fn claim_message(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, msg_id)): Path<(String, String)>,
    Json(body): Json<ClaimMessageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let agent_id = AgentId::from_str(&body.agent_id)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let message_id = MessageId::from_str(&msg_id)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    container
        .app
        .claim_message
        .execute(agent_id, message_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn unclaim_message(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, msg_id)): Path<(String, String)>,
    Json(body): Json<ClaimMessageBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let agent_id = AgentId::from_str(&body.agent_id)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let message_id = MessageId::from_str(&msg_id)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    container
        .app
        .unclaim_message
        .execute(agent_id, message_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct ClaimMessageBody {
    pub agent_id: String,
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), ApiError> {
    if auth.org.id.as_str() != org_id.as_str() {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ))
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct AgentNamespaceQuery {
    pub namespace: Option<String>,
    pub project: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct SendBody {
    #[serde(alias = "from_agent_id")]
    pub from_alias: String,
    pub to: String,
    pub body: String,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
    pub reply_to: Option<String>,
    pub refs: Option<Vec<orchy_core::resource_ref::ResourceRef>>,
}

#[derive(Deserialize)]
pub struct MarkReadBody {
    pub message_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct ThreadQuery {
    pub limit: Option<u32>,
}

async fn resolve_agent_id_for_messages(
    container: &Arc<Container>,
    org: &OrganizationId,
    id: &str,
    project: Option<&str>,
) -> Result<(String, String), ApiError> {
    if let Ok(agent_id) = id.parse::<orchy_core::agent::AgentId>() {
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
        return Ok((agent_id.to_string(), agent.project().to_string()));
    }
    let project = project.ok_or_else(|| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            "project query param is required when addressing agent by alias".to_string(),
        )
    })?;
    let project_id = ProjectId::try_from(project.to_string())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    let agent = resolve_agent(container.agent_store.as_ref(), org, &project_id, id)
        .await
        .map_err(ApiError::from)?;
    Ok((agent.id().to_string(), agent.project().to_string()))
}

pub async fn inbox_for_agent(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<AgentNamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let (agent_id, project) =
        resolve_agent_id_for_messages(&container, &org_id, &id, query.project.as_deref()).await?;

    let cmd = CheckMailboxCommand {
        agent_id,
        org_id: org,
        project,
        after: query.after,
        limit: query.limit,
    };

    let page = container
        .app
        .check_mailbox
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let v = serde_json::to_value(&page).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        )
    })?;
    Ok(Json(v))
}

pub async fn sent_for_agent(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<AgentNamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let (agent_id, project) =
        resolve_agent_id_for_messages(&container, &org_id, &id, query.project.as_deref()).await?;

    let cmd = CheckSentMessagesCommand {
        agent_id,
        org_id: org,
        project,
        namespace: query.namespace,
        after: query.after,
        limit: query.limit,
    };

    let page = container
        .app
        .check_sent_messages
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let v = serde_json::to_value(&page).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        )
    })?;
    Ok(Json(v))
}

pub async fn send(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<SendBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = SendMessageCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        from_agent_id: body.from_alias,
        to: body.to,
        body: body.body,
        reply_to: body.reply_to,
        refs: body.refs.unwrap_or_default(),
    };

    let message = container
        .app
        .send_message
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let v = serde_json::to_value(&message).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        )
    })?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct MarkReadQuery {
    pub project: Option<String>,
}

pub async fn mark_read(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<MarkReadQuery>,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let (agent_id, _) =
        resolve_agent_id_for_messages(&container, &org_id, &id, query.project.as_deref()).await?;

    let cmd = MarkReadCommand {
        agent_id,
        message_ids: body.message_ids,
    };

    container
        .app
        .mark_read
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn thread(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, msg_id)): Path<(String, String, String)>,
    Query(query): Query<ThreadQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ListConversationCommand {
        org_id: org,
        project,
        message_id: msg_id,
        limit: query.limit,
    };

    let messages = container
        .app
        .list_conversation
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    let v = serde_json::to_value(&messages).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        )
    })?;
    Ok(Json(v))
}
