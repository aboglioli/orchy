pub mod agents;
pub mod auth;
pub mod events;
pub mod graph;
pub mod knowledge;
pub mod locks;
pub mod messages;
pub mod middleware;
pub mod orgs;
pub mod projects;
pub mod tasks;
pub mod user_auth;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use orchy_core::graph::RelationOptions;
use orchy_core::graph::{RelationType, TraversalDirection};
use orchy_core::namespace::Namespace;

use crate::container::Container;

pub(crate) fn parse_namespace(s: &str) -> Result<Namespace, ApiError> {
    let normalized = if s.is_empty() || s == "/" {
        "/".to_string()
    } else if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{s}")
    };
    Namespace::try_from(normalized).map_err(|e| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            format!("invalid namespace: {e}"),
        )
    })
}

#[derive(Deserialize, Default)]
pub struct InlineRelationQuery {
    pub rel_types: Option<String>,
    pub direction: Option<String>,
    pub max_depth: Option<u32>,
}

impl InlineRelationQuery {
    pub fn into_options(self) -> Result<Option<RelationOptions>, ApiError> {
        if self.rel_types.is_none() && self.direction.is_none() && self.max_depth.is_none() {
            return Ok(None);
        }

        let rel_types: Option<Vec<RelationType>> = self
            .rel_types
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().parse::<RelationType>())
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?;

        let direction = match self.direction.as_deref() {
            Some("outgoing") => TraversalDirection::Outgoing,
            Some("incoming") => TraversalDirection::Incoming,
            _ => TraversalDirection::Both,
        };

        Ok(Some(RelationOptions {
            rel_types,
            target_kinds: vec![],
            direction,
            max_depth: self.max_depth.unwrap_or(1),
            limit: 50,
        }))
    }
}

#[derive(Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

pub struct ApiError(pub StatusCode, pub &'static str, pub String);

impl From<orchy_core::error::Error> for ApiError {
    fn from(e: orchy_core::error::Error) -> Self {
        use orchy_core::error::Error;
        match &e {
            Error::NotFound(_) => ApiError(StatusCode::NOT_FOUND, "NOT_FOUND", e.to_string()),
            Error::InvalidInput(_)
            | Error::InvalidTransition { .. }
            | Error::DependencyNotMet(_) => ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_INPUT",
                e.to_string(),
            ),
            Error::Conflict(_) | Error::VersionMismatch { .. } => {
                ApiError(StatusCode::CONFLICT, "CONFLICT", e.to_string())
            }
            Error::Embeddings(_) => {
                ApiError(StatusCode::BAD_GATEWAY, "EMBEDDINGS_ERROR", e.to_string())
            }
            Error::Store(_) => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                e.to_string(),
            ),
            Error::AuthenticationFailed(_) => {
                ApiError(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", e.to_string())
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let body = ApiErrorEnvelope {
            error: ApiErrorBody {
                code: self.1,
                message: self.2,
            },
        };
        (self.0, Json(body)).into_response()
    }
}

pub fn router() -> Router<Arc<Container>> {
    use axum::routing::{delete, get, post};

    Router::new()
        .route("/auth/register", post(user_auth::register))
        .route("/auth/login", post(user_auth::login))
        .route("/auth/logout", post(user_auth::logout))
        .route("/auth/me", get(user_auth::me))
        .route("/auth/change-password", post(user_auth::change_password))
        .route("/organizations", post(orgs::create).get(orgs::list))
        .route("/organizations/{org}", get(orgs::get))
        .route("/organizations/{org}/invite", post(user_auth::invite_user))
        .route("/api-keys", post(orgs::add_api_key))
        .route("/api-keys/{key_id}", delete(orgs::revoke_api_key))
        .route("/agents", get(agents::list))
        .route("/projects/{project}/agents", post(agents::register))
        .route("/agents/{id}/context", get(agents::get_context))
        .route("/agents/{id}/summary", get(agents::get_summary))
        .route(
            "/agents/{id}/roles",
            axum::routing::patch(agents::change_roles),
        )
        .route("/agents/{id}/rename", post(agents::rename_alias))
        .route("/agents/{id}/switch-context", post(agents::switch_context))
        .route("/agents/{id}/inbox", get(messages::inbox_for_agent))
        .route("/agents/{id}/sent-messages", get(messages::sent_for_agent))
        .route("/agents/{id}/messages/read", post(messages::mark_read))
        .route("/messages/{msg_id}/claim", post(messages::claim_message))
        .route(
            "/messages/{msg_id}/unclaim",
            post(messages::unclaim_message),
        )
        .route(
            "/projects/{project}",
            get(projects::get).put(projects::update),
        )
        .route("/projects/{project}/metadata", post(projects::set_metadata))
        .route(
            "/projects/{project}/namespaces",
            get(projects::list_namespaces),
        )
        .route(
            "/projects/{project}/tasks",
            get(tasks::list).post(tasks::post),
        )
        .route("/projects/{project}/tasks/next", get(tasks::next_task))
        .route("/projects/{project}/tasks/tags", get(tasks::list_tags))
        .route("/projects/{project}/tasks/merge", post(tasks::merge))
        .route(
            "/projects/{project}/tasks/{id}",
            get(tasks::get_task).patch(tasks::update_task),
        )
        .route("/projects/{project}/tasks/{id}/claim", post(tasks::claim))
        .route("/projects/{project}/tasks/{id}/start", post(tasks::start))
        .route(
            "/projects/{project}/tasks/{id}/complete",
            post(tasks::complete),
        )
        .route("/projects/{project}/tasks/{id}/fail", post(tasks::fail))
        .route("/projects/{project}/tasks/{id}/cancel", post(tasks::cancel))
        .route(
            "/projects/{project}/tasks/{id}/release",
            post(tasks::release),
        )
        .route(
            "/projects/{project}/tasks/{id}/archive",
            post(tasks::archive_task),
        )
        .route(
            "/projects/{project}/tasks/{id}/unarchive",
            post(tasks::unarchive_task),
        )
        .route(
            "/projects/{project}/tasks/{id}/unblock",
            post(tasks::unblock),
        )
        .route("/projects/{project}/tasks/{id}/assign", post(tasks::assign))
        .route(
            "/projects/{project}/tasks/{id}/dependencies",
            post(tasks::add_dep),
        )
        .route(
            "/projects/{project}/tasks/{id}/dependencies/{dep_id}",
            delete(tasks::remove_dep),
        )
        .route(
            "/projects/{project}/tasks/{id}/tags/{tag}",
            post(tasks::tag_task).delete(tasks::untag_task),
        )
        .route(
            "/projects/{project}/tasks/{id}/move",
            post(tasks::move_task),
        )
        .route("/projects/{project}/tasks/{id}/split", post(tasks::split))
        .route(
            "/projects/{project}/tasks/{id}/replace",
            post(tasks::replace),
        )
        .route(
            "/projects/{project}/tasks/{id}/delegate",
            post(tasks::delegate),
        )
        .route("/projects/{project}/tasks/{id}/touch", post(tasks::touch))
        .route("/projects/{project}/messages", post(messages::send))
        .route(
            "/projects/{project}/messages/{id}/thread",
            get(messages::thread),
        )
        .route("/projects/{project}/knowledge", get(knowledge::list))
        // Graph endpoints
        .route("/graph/edges", post(graph::add_edge).get(graph::list_edges))
        .route("/graph/edges/{edge_id}", delete(graph::remove_edge))
        .route("/graph/relations", get(graph::query_relations))
        .route("/graph/context", post(graph::assemble_context))
        .route(
            "/projects/{project}/knowledge/types",
            get(knowledge::list_types),
        )
        .route(
            "/projects/{project}/knowledge/search",
            post(knowledge::search),
        )
        .route(
            "/projects/{project}/knowledge/import",
            post(knowledge::import),
        )
        .route(
            "/projects/{project}/knowledge/promote",
            post(knowledge::promote),
        )
        .route(
            "/projects/{project}/knowledge/consolidate",
            post(knowledge::consolidate),
        )
        .route(
            "/projects/{project}/knowledge/{*path}",
            get(knowledge::read)
                .put(knowledge::write)
                .delete(knowledge::delete)
                .post(knowledge::knowledge_action)
                .patch(knowledge::knowledge_patch),
        )
        .route("/projects/{project}/locks", post(locks::acquire))
        .route(
            "/projects/{project}/locks/{name}",
            get(locks::check).delete(locks::release),
        )
        .route("/projects/{project}/events", get(events::poll))
}
