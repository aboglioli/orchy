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
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), ApiError> {
    if auth.0.id.as_str() != org_id.as_str() {
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
    pub after: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct SendBody {
    pub from_agent_id: String,
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
        .execute(orchy_application::GetAgentCommand {
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

    let cmd = CheckMailboxCommand {
        agent_id: id,
        org_id: org,
        project: agent.project.clone(),
        namespace: query.namespace,
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

    let agent = container
        .app
        .get_agent
        .execute(orchy_application::GetAgentCommand {
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

    let cmd = CheckSentMessagesCommand {
        agent_id: id,
        org_id: org,
        project: agent.project.clone(),
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
        from_agent_id: body.from_agent_id,
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
        .execute(orchy_application::GetAgentCommand {
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
