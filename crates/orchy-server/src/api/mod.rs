pub mod agents;
pub mod auth;
pub mod events;
pub mod knowledge;
pub mod locks;
pub mod messages;
pub mod orgs;
pub mod projects;
pub mod reviews;
pub mod tasks;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::container::Container;

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
            "/organizations/:org/projects/:project/agents/:alias/context",
            get(agents::get_context),
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
            "/organizations/:org/projects/:project/tasks/:id/watch",
            post(tasks::watch).delete(tasks::unwatch),
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
            "/organizations/:org/projects/:project/tasks/:id/reviews",
            get(reviews::list_for_task).post(reviews::request),
        )
        .route(
            "/organizations/:org/projects/:project/reviews/:id",
            get(reviews::get).put(reviews::resolve),
        )
        .route(
            "/organizations/:org/projects/:project/messages/inbox",
            get(messages::inbox),
        )
        .route(
            "/organizations/:org/projects/:project/messages/sent",
            get(messages::sent),
        )
        .route(
            "/organizations/:org/projects/:project/messages",
            post(messages::send),
        )
        .route(
            "/organizations/:org/projects/:project/messages/read",
            post(messages::mark_read),
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
