use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_core::agent::AgentId;
use orchy_core::message::{Message, MessageId, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

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
        Some(s) => Namespace::try_from(format!("/{s}"))
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string())),
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

fn map_err(e: orchy_core::error::Error) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        e.to_string(),
    )
}

#[derive(Deserialize)]
pub struct InboxQuery {
    pub agent_id: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct SentQuery {
    pub agent_id: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct SendBody {
    pub from_agent_id: String,
    pub to: String,
    pub body: String,
    pub ns: Option<String>,
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

pub async fn inbox(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Query(query): Query<InboxQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let agent_id = query
        .agent_id
        .parse::<AgentId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let messages = container
        .message_service
        .check(&agent_id, &org_id, &project_id, &ns)
        .await
        .map_err(map_err)?;

    Ok(Json(messages))
}

pub async fn sent(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Query(query): Query<SentQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let agent_id = query
        .agent_id
        .parse::<AgentId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let messages = container
        .message_service
        .sent(&agent_id, &org_id, &project_id, &ns)
        .await
        .map_err(map_err)?;

    Ok(Json(messages))
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
    let ns = parse_ns(body.ns.as_deref())?;

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
        .send(
            org_id,
            project_id,
            ns,
            from_agent_id,
            target,
            body.body,
            reply_to,
        )
        .await
        .map_err(map_err)?;

    Ok(Json(messages))
}

pub async fn mark_read(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project)): Path<(String, String)>,
    Json(body): Json<MarkReadBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let ids: Vec<MessageId> = body
        .message_ids
        .iter()
        .map(|s| {
            s.parse::<MessageId>()
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    container
        .message_service
        .mark_read(&ids)
        .await
        .map_err(map_err)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn thread(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project, msg_id)): Path<(String, String, String)>,
    Query(query): Query<ThreadQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let message_id = msg_id
        .parse::<MessageId>()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let limit = query.limit.map(|n| n as usize);

    let messages = container
        .message_service
        .thread(&message_id, limit)
        .await
        .map_err(map_err)?;

    Ok(Json(messages))
}
