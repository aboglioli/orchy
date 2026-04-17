use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_application::{
    CheckMailboxCommand, CheckSentMessagesCommand, ListConversationCommand, MarkReadCommand,
    SendMessageCommand,
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
pub struct AgentNamespaceQuery {
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct SendBody {
    pub from_agent_id: String,
    pub to: String,
    pub body: String,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
    pub reply_to: Option<String>,
}

#[derive(Deserialize)]
pub struct MarkReadBody {
    pub message_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct ThreadQuery {
    pub limit: Option<u32>,
}

pub async fn inbox_for_agent(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<AgentNamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let agent = container
        .app
        .get_agent
        .execute(&id)
        .await
        .map_err(ApiError::from)?;

    if agent.org_id() != &org_id || agent.status() == orchy_core::agent::AgentStatus::Disconnected {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    let cmd = CheckMailboxCommand {
        agent_id: id,
        org_id: org,
        project: agent.project().to_string(),
        namespace: query.namespace,
        after: None,
        limit: None,
    };

    let page = container
        .app
        .check_mailbox
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&page.items).unwrap_or_default()))
}

pub async fn sent_for_agent(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<AgentNamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let agent = container
        .app
        .get_agent
        .execute(&id)
        .await
        .map_err(ApiError::from)?;

    if agent.org_id() != &org_id || agent.status() == orchy_core::agent::AgentStatus::Disconnected {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    let cmd = CheckSentMessagesCommand {
        agent_id: id,
        org_id: org,
        project: agent.project().to_string(),
        namespace: query.namespace,
        after: None,
        limit: None,
    };

    let page = container
        .app
        .check_sent_messages
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&page.items).unwrap_or_default()))
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
        from_agent_id: body.from_agent_id,
        to: body.to,
        body: body.body,
        reply_to: body.reply_to,
        refs: None,
    };

    let messages = container
        .app
        .send_message
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&messages).unwrap_or_default()))
}

pub async fn mark_read_for_agent(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let agent = container
        .app
        .get_agent
        .execute(&id)
        .await
        .map_err(ApiError::from)?;

    if agent.org_id() != &org_id || agent.status() == orchy_core::agent::AgentStatus::Disconnected {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    let cmd = MarkReadCommand {
        agent_id: id,
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
    let project_id = ProjectId::try_from(project)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let cmd = ListConversationCommand {
        message_id: msg_id.clone(),
        limit: query.limit,
    };

    let messages = container
        .app
        .list_conversation
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    if let Some(first) = messages.first()
        && first.project() != &project_id
    {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("message {msg_id} not found in project {project_id}"),
        ));
    }

    Ok(Json(serde_json::to_value(&messages).unwrap_or_default()))
}
