pub mod agents;
pub mod auth;
pub mod events;
pub mod knowledge;
pub mod locks;
pub mod messages;
pub mod orgs;
pub mod projects;
pub mod tasks;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

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
    use axum::routing::{delete, get, patch, post};

    Router::new()
        .route("/organizations", post(orgs::create).get(orgs::list))
        .route("/organizations/:org", get(orgs::get))
        .route("/organizations/:org/api-keys", post(orgs::add_api_key))
        .route(
            "/organizations/:org/api-keys/:key_id",
            delete(orgs::revoke_api_key),
        )
        .route("/organizations/:org/agents", get(agents::list))
        .route(
            "/organizations/:org/agents/:id/context",
            get(agents::get_context),
        )
        .route(
            "/organizations/:org/agents/:id/summary",
            get(agents::get_summary),
        )
        .route(
            "/organizations/:org/agents/:id/inbox",
            get(messages::inbox_for_agent),
        )
        .route(
            "/organizations/:org/agents/:id/sent-messages",
            get(messages::sent_for_agent),
        )
        .route(
            "/organizations/:org/agents/:id/messages/read",
            post(messages::mark_read_for_agent),
        )
        .route(
            "/organizations/:org/projects/:project",
            get(projects::get).put(projects::update),
        )
        .route(
            "/organizations/:org/projects/:project/metadata",
            post(projects::set_metadata),
        )
        .route(
            "/organizations/:org/projects/:project/namespaces",
            get(projects::list_namespaces),
        )
        .route(
            "/organizations/:org/projects/:project/tasks",
            get(tasks::list).post(tasks::post),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/next",
            get(tasks::next_task),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/tags",
            get(tasks::list_tags),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/merge",
            post(tasks::merge),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id",
            get(tasks::get_task).patch(tasks::update_task),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/claim",
            post(tasks::claim),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/start",
            post(tasks::start),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/complete",
            post(tasks::complete),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/fail",
            post(tasks::fail),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/cancel",
            post(tasks::cancel),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/release",
            post(tasks::release),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/unblock",
            post(tasks::unblock),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/assign",
            post(tasks::assign),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/notes",
            post(tasks::add_note),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/dependencies",
            post(tasks::add_dep),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/dependencies/:dep_id",
            delete(tasks::remove_dep),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/tags/:tag",
            post(tasks::tag_task).delete(tasks::untag_task),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/split",
            post(tasks::split),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/replace",
            post(tasks::replace),
        )
        .route(
            "/organizations/:org/projects/:project/tasks/:id/delegate",
            post(tasks::delegate),
        )
        .route(
            "/organizations/:org/projects/:project/messages",
            post(messages::send),
        )
        .route(
            "/organizations/:org/projects/:project/messages/:id/thread",
            get(messages::thread),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge",
            get(knowledge::list),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/types",
            get(knowledge::list_types),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/search",
            post(knowledge::search),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/import",
            post(knowledge::import),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path",
            get(knowledge::read)
                .put(knowledge::write)
                .delete(knowledge::delete),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path/append",
            post(knowledge::append),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path/move",
            post(knowledge::move_entry),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path/rename",
            post(knowledge::rename),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path/kind",
            patch(knowledge::change_kind),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path/metadata",
            patch(knowledge::patch_metadata),
        )
        .route(
            "/organizations/:org/projects/:project/knowledge/*path/tags/:tag",
            post(knowledge::tag).delete(knowledge::untag),
        )
        .route(
            "/organizations/:org/projects/:project/locks",
            post(locks::acquire),
        )
        .route(
            "/organizations/:org/projects/:project/locks/:name",
            get(locks::check).delete(locks::release),
        )
        .route(
            "/organizations/:org/projects/:project/events",
            get(events::poll),
        )
}
