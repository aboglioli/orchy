use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_core::agent::{AgentId, AgentStatus};
use orchy_core::message::service::SendMessage;
use orchy_core::message::{Message, MessageId, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::auth::OrgAuth;
use super::{ApiError, parse_namespace};

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_project(s: &str) -> Result<ProjectId, ApiError> {
    ProjectId::try_from(s.to_string())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_ns(ns: Option<&str>) -> Result<Namespace, ApiError> {
    match ns {
        Some(s) => parse_namespace(s),
        None => Ok(Namespace::root()),
    }
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

async fn load_agent(
    container: &Arc<Container>,
    auth: &OrgAuth,
    org: &str,
    id: &str,
) -> Result<orchy_core::agent::Agent, ApiError> {
    let org_id = parse_org(org)?;
    check_org(auth, &org_id)?;

    let agent_id = id
        .parse::<AgentId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let agent = container
        .agent_service
        .get(&agent_id)
        .await
        .map_err(ApiError::from)?;

    if agent.org_id() != &org_id || agent.status() == AgentStatus::Disconnected {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "agent not found".to_string(),
        ));
    }

    Ok(agent)
}

pub async fn inbox_for_agent(
    state: State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<AgentNamespaceQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let agent = load_agent(&state.0, &auth, &org, &id).await?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let page = state
        .0
        .message_service
        .check(
            agent.id(),
            agent.org_id(),
            agent.project(),
            &ns,
            orchy_core::pagination::PageParams::unbounded(),
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(page.items))
}

pub async fn sent_for_agent(
    state: State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Query(query): Query<AgentNamespaceQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let agent = load_agent(&state.0, &auth, &org, &id).await?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let page = state
        .0
        .message_service
        .sent(
            agent.id(),
            agent.org_id(),
            agent.project(),
            &ns,
            orchy_core::pagination::PageParams::unbounded(),
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(page.items))
}

pub async fn send(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<SendBody>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(body.namespace.as_deref())?;

    let from_agent_id = body
        .from_agent_id
        .parse::<AgentId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let target = MessageTarget::parse(&body.to)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let reply_to = body
        .reply_to
        .as_deref()
        .map(|s| {
            s.parse::<MessageId>()
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
        })
        .transpose()?;

    let messages = container
        .message_service
        .send(SendMessage {
            org_id,
            project: project_id,
            namespace: ns,
            from: from_agent_id,
            to: target,
            body: body.body,
            reply_to,
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(messages))
}

pub async fn mark_read_for_agent(
    state: State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, id)): Path<(String, String)>,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let agent = load_agent(&state.0, &auth, &org, &id).await?;

    let ids: Vec<MessageId> = body
        .message_ids
        .iter()
        .map(|s| {
            s.parse::<MessageId>()
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    state
        .0
        .message_service
        .mark_read(agent.id(), &ids)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn thread(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, msg_id)): Path<(String, String, String)>,
    Query(query): Query<ThreadQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;

    let message_id = msg_id
        .parse::<MessageId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let limit = query.limit.map(|n| n as usize);

    let messages = container
        .message_service
        .thread(&message_id, limit)
        .await
        .map_err(ApiError::from)?;

    if let Some(first) = messages.first()
        && first.project() != &project_id
    {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("message {message_id} not found in project {project_id}"),
        ));
    }

    Ok(Json(messages))
}
