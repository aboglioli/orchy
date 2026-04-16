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

use axum::Router;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

use orchy_core::agent::AgentStatus;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

pub fn router() -> Router<Arc<Container>> {
    use axum::routing::{delete, get, patch, post};

    Router::new()
        .route("/agents", get(list_agents))
        .route("/pending", get(pending_work))
        .route("/orgs", post(orgs::create).get(orgs::list))
        .route("/orgs/:org", get(orgs::get))
        .route("/orgs/:org/api-keys", post(orgs::add_api_key))
        .route("/orgs/:org/api-keys/:key_id", delete(orgs::revoke_api_key))
        .route("/:org/agents", get(agents::list))
        .route(
            "/:org/projects/:project",
            get(projects::get).put(projects::update),
        )
        .route(
            "/:org/projects/:project/metadata",
            post(projects::set_metadata),
        )
        .route(
            "/:org/projects/:project/namespaces",
            get(projects::list_namespaces),
        )
        .route("/:org/projects/:project/overview", get(projects::overview))
        .route("/:org/:project/tasks", get(tasks::list).post(tasks::post))
        .route("/:org/:project/tasks/next", get(tasks::next_task))
        .route("/:org/:project/tasks/tags", get(tasks::list_tags))
        .route("/:org/:project/tasks/merge", post(tasks::merge))
        .route(
            "/:org/:project/tasks/:id",
            get(tasks::get_task).patch(tasks::update_task),
        )
        .route("/:org/:project/tasks/:id/claim", post(tasks::claim))
        .route("/:org/:project/tasks/:id/start", post(tasks::start))
        .route("/:org/:project/tasks/:id/complete", post(tasks::complete))
        .route("/:org/:project/tasks/:id/fail", post(tasks::fail))
        .route("/:org/:project/tasks/:id/cancel", post(tasks::cancel))
        .route("/:org/:project/tasks/:id/release", post(tasks::release))
        .route("/:org/:project/tasks/:id/unblock", post(tasks::unblock))
        .route("/:org/:project/tasks/:id/assign", post(tasks::assign))
        .route(
            "/:org/:project/tasks/:id/watch",
            post(tasks::watch).delete(tasks::unwatch),
        )
        .route("/:org/:project/tasks/:id/notes", post(tasks::add_note))
        .route("/:org/:project/tasks/:id/deps", post(tasks::add_dep))
        .route(
            "/:org/:project/tasks/:id/deps/:dep_id",
            delete(tasks::remove_dep),
        )
        .route(
            "/:org/:project/tasks/:id/tags/:tag",
            post(tasks::tag_task).delete(tasks::untag_task),
        )
        .route("/:org/:project/tasks/:id/split", post(tasks::split))
        .route("/:org/:project/tasks/:id/replace", post(tasks::replace))
        .route("/:org/:project/tasks/:id/delegate", post(tasks::delegate))
        .route(
            "/:org/:project/tasks/:id/reviews",
            get(reviews::list_for_task).post(reviews::request),
        )
        .route(
            "/:org/:project/reviews/:id",
            get(reviews::get).put(reviews::resolve),
        )
        .route("/:org/:project/messages/inbox", get(messages::inbox))
        .route("/:org/:project/messages/sent", get(messages::sent))
        .route("/:org/:project/messages", post(messages::send))
        .route("/:org/:project/messages/read", post(messages::mark_read))
        .route("/:org/:project/messages/:id/thread", get(messages::thread))
        .route("/:org/:project/knowledge", get(knowledge::list))
        .route("/:org/:project/knowledge-types", get(knowledge::list_types))
        .route("/:org/:project/knowledge/search", post(knowledge::search))
        .route("/:org/:project/knowledge/import", post(knowledge::import))
        .route(
            "/:org/:project/knowledge/*path",
            get(knowledge::read)
                .put(knowledge::write)
                .delete(knowledge::delete),
        )
        .route(
            "/:org/:project/knowledge/*path/append",
            post(knowledge::append),
        )
        .route(
            "/:org/:project/knowledge/*path/move",
            post(knowledge::move_entry),
        )
        .route(
            "/:org/:project/knowledge/*path/rename",
            post(knowledge::rename),
        )
        .route(
            "/:org/:project/knowledge/*path/kind",
            patch(knowledge::change_kind),
        )
        .route(
            "/:org/:project/knowledge/*path/metadata",
            patch(knowledge::patch_metadata),
        )
        .route(
            "/:org/:project/knowledge/*path/tags/:tag",
            post(knowledge::tag).delete(knowledge::untag),
        )
        .route("/:org/:project/locks", post(locks::acquire))
        .route(
            "/:org/:project/locks/:name",
            get(locks::check).delete(locks::release),
        )
        .route("/:org/:project/events", get(events::poll))
}

#[derive(Deserialize)]
pub struct ListAgentsQuery {
    pub project: String,
}

#[derive(Serialize)]
pub struct AgentDto {
    pub id: String,
    pub alias: Option<String>,
    pub description: String,
    pub status: String,
    pub agent_type: Option<String>,
    pub namespace: String,
    pub last_heartbeat: String,
}

pub async fn list_agents(
    State(container): State<Arc<Container>>,
    params: Query<ListAgentsQuery>,
) -> impl IntoResponse {
    let project = match ProjectId::try_from(params.project.clone()) {
        Ok(p) => p,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let org = OrganizationId::new("default").unwrap();
    match container.agent_service.list(&org).await {
        Ok(agents) => {
            let body: Vec<AgentDto> = agents
                .into_iter()
                .filter(|a| *a.project() == project && a.status() != AgentStatus::Disconnected)
                .map(|a| AgentDto {
                    id: a.id().to_string(),
                    alias: a.alias().map(|al| al.to_string()),
                    description: a.description().to_string(),
                    status: a.status().to_string(),
                    agent_type: a.metadata().get("agent_type").cloned(),
                    namespace: a.namespace().to_string(),
                    last_heartbeat: a.last_heartbeat().to_rfc3339(),
                })
                .collect();
            axum::Json(body).into_response()
        }
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
pub struct PendingWorkQuery {
    pub project: String,
    pub alias: String,
}

#[derive(Serialize)]
pub struct PendingWorkDto {
    pub messages: Vec<PendingMessageDto>,
    pub tasks: Vec<PendingTaskDto>,
    pub reviews: Vec<PendingReviewDto>,
}

#[derive(Serialize)]
pub struct PendingMessageDto {
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

#[derive(Serialize)]
pub struct PendingReviewDto {
    pub id: String,
    pub task_id: String,
}

pub async fn pending_work(
    State(container): State<Arc<Container>>,
    params: Query<PendingWorkQuery>,
) -> impl IntoResponse {
    let project_id = match ProjectId::try_from(params.project.clone()) {
        Ok(p) => p,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let org = OrganizationId::new("default").unwrap();

    let agent = match container
        .agent_service
        .find_by_alias_str(&org, &project_id, &params.alias)
        .await
    {
        Ok(Some(a)) if a.status() != AgentStatus::Disconnected => a,
        Ok(_) | Err(_) => {
            return axum::Json(PendingWorkDto {
                messages: Vec::new(),
                tasks: Vec::new(),
                reviews: Vec::new(),
            })
            .into_response();
        }
    };

    let messages = container
        .message_service
        .pending(&agent.id(), &org, agent.project(), agent.namespace())
        .await
        .map(|msgs| {
            msgs.into_iter()
                .map(|m| PendingMessageDto {
                    id: m.id().to_string(),
                    from: m.from().to_string(),
                    body: m.body().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let tasks = container
        .task_service
        .pending_tasks_for_roles(agent.roles(), Some(agent.namespace().clone()))
        .await
        .map(|tasks| {
            tasks
                .into_iter()
                .take(5)
                .map(|t| PendingTaskDto {
                    id: t.id().to_string(),
                    title: t.title().to_string(),
                    priority: t.priority().to_string(),
                    assigned_roles: t.assigned_roles().to_vec(),
                })
                .collect()
        })
        .unwrap_or_default();

    let reviews = container
        .task_service
        .pending_reviews_for_agent(&agent.id())
        .await
        .map(|reviews| {
            reviews
                .into_iter()
                .map(|r| PendingReviewDto {
                    id: r.id().to_string(),
                    task_id: r.task_id().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    axum::Json(PendingWorkDto {
        messages,
        tasks,
        reviews,
    })
    .into_response()
}
