use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_application::{
    AddEdgeCommand, AssembleContextCommand, ListEdgesCommand, MaterializeNeighborhoodCommand,
    RemoveEdgeCommand,
};
use orchy_core::graph::{RelationOptions, RelationType, TraversalDirection};

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct AddEdgeBody {
    pub from_kind: String,
    pub from_id: String,
    pub to_kind: String,
    pub to_id: String,
    pub rel_type: String,
    pub if_not_exists: Option<bool>,
    pub agent_id: Option<String>,
}

pub async fn add_edge(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Json(body): Json<AddEdgeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let cmd = AddEdgeCommand {
        org_id: org,
        from_kind: body.from_kind,
        from_id: body.from_id,
        to_kind: body.to_kind,
        to_id: body.to_id,
        rel_type: body.rel_type,
        created_by: body.agent_id,
        if_not_exists: body.if_not_exists.unwrap_or(true),
    };

    let edge = container
        .app
        .add_edge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&edge).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn remove_edge(
    State(container): State<Arc<Container>>,
    _auth: OrgAuth,
    Path(edge_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cmd = RemoveEdgeCommand {
        edge_id: edge_id.clone(),
    };

    container
        .app
        .remove_edge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true, "edge_id": edge_id})))
}

#[derive(Deserialize)]
pub struct QueryRelationsQuery {
    pub anchor_kind: String,
    pub anchor_id: String,
    pub rel_types: Option<String>,
    pub direction: Option<String>,
    pub max_depth: Option<u32>,
    pub as_of: Option<String>,
    pub project: Option<String>,
}

pub async fn query_relations(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Query(query): Query<QueryRelationsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let rel_types: Option<Vec<RelationType>> = query
        .rel_types
        .as_deref()
        .map(|s| {
            s.split(',')
                .map(|t| t.trim().parse::<RelationType>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?;

    let direction = match query.direction.as_deref() {
        Some("outgoing") => TraversalDirection::Outgoing,
        Some("incoming") => TraversalDirection::Incoming,
        _ => TraversalDirection::Both,
    };

    let options = RelationOptions {
        rel_types,
        target_kinds: vec![],
        direction,
        max_depth: query.max_depth.unwrap_or(1),
        limit: 200,
    };

    let as_of = query
        .as_of
        .as_deref()
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s).map_err(|e| {
                ApiError(
                    StatusCode::BAD_REQUEST,
                    "INVALID_PARAM",
                    format!("invalid as_of timestamp: {e}"),
                )
            })
        })
        .transpose()?
        .map(|dt| dt.to_utc());

    let cmd = MaterializeNeighborhoodCommand {
        org_id: org,
        anchor_kind: query.anchor_kind,
        anchor_id: query.anchor_id,
        options,
        as_of,
        project: query.project,
        namespace: None,
        semantic_query: None,
    };

    let neighborhood = container
        .app
        .materialize_neighborhood
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&neighborhood).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

#[derive(Deserialize)]
pub struct ListEdgesQuery {
    pub from_kind: Option<String>,
    pub from_id: Option<String>,
    pub to_kind: Option<String>,
    pub to_id: Option<String>,
    pub rel_type: Option<String>,
    pub as_of: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub async fn list_edges(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Query(query): Query<ListEdgesQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let as_of = query
        .as_of
        .as_deref()
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s).map_err(|e| {
                ApiError(
                    StatusCode::BAD_REQUEST,
                    "INVALID_PARAM",
                    format!("invalid as_of timestamp: {e}"),
                )
            })
        })
        .transpose()?
        .map(|dt| dt.to_utc());

    let page = container
        .app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: auth.org.id.clone(),
            rel_type: query.rel_type.clone(),
            after: query.after.clone(),
            limit: query.limit,
            as_of,
        })
        .await
        .map_err(ApiError::from)?;

    // If from/to filters are provided, filter in-memory since the store only
    // supports rel_type filter natively. This is an admin/debug endpoint.
    let items: Vec<_> = page
        .items
        .iter()
        .filter(|e| {
            query
                .from_kind
                .as_deref()
                .map(|fk| e.from_kind == fk)
                .unwrap_or(true)
                && query
                    .from_id
                    .as_deref()
                    .map(|fi| e.from_id == fi)
                    .unwrap_or(true)
                && query
                    .to_kind
                    .as_deref()
                    .map(|tk| e.to_kind == tk)
                    .unwrap_or(true)
                && query
                    .to_id
                    .as_deref()
                    .map(|ti| e.to_id == ti)
                    .unwrap_or(true)
        })
        .cloned()
        .collect();

    Ok(Json(serde_json::json!({
        "items": items,
        "next_cursor": page.next_cursor,
    })))
}

#[derive(Deserialize)]
pub struct AssembleContextBody {
    pub kind: String,
    pub id: String,
    pub max_tokens: Option<usize>,
}

pub async fn assemble_context(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Json(body): Json<AssembleContextBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org = auth.org.id.clone();
    let cmd = AssembleContextCommand {
        org_id: org,
        kind: body.kind,
        id: body.id,
        max_tokens: body.max_tokens,
    };

    let context = container
        .app
        .assemble_context
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&context).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}
