use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use orchy_core::task::ReviewStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use orchy_core::agent::AgentStatus;
use orchy_core::message::MessageStore;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::task::{TaskStatus, TaskStore};

use crate::container::Container;

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
    Query(params): Query<ListAgentsQuery>,
) -> impl IntoResponse {
    let org = OrganizationId::new("default").unwrap();

    let project_id = match ProjectId::try_from(params.project) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    match container.agent_service.list(&org).await {
        Ok(agents) => {
            let body: Vec<AgentDto> = agents
                .into_iter()
                .filter(|a| *a.project() == project_id && a.status() != AgentStatus::Disconnected)
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
            Json(body).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
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
    Query(params): Query<PendingWorkQuery>,
) -> impl IntoResponse {
    let org = OrganizationId::new("default").unwrap();

    let project_id = match ProjectId::try_from(params.project) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let agents = match container.agent_service.list(&org) {
        Ok(a) => a,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let agents = match agents.await {
        Ok(a) => a,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let Some(agent) = agents.into_iter().find(|a| {
        *a.project() == project_id
            && a.status() != AgentStatus::Disconnected
            && a.alias().map(|al| al.to_string()).as_deref() == Some(&params.alias)
    }) else {
        return Json(PendingWorkDto {
            messages: Vec::new(),
            tasks: Vec::new(),
            reviews: Vec::new(),
        })
        .into_response();
    };

    let messages = container
        .store
        .find_pending(&agent.id(), &org, &project_id, agent.namespace())
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
        .list_tasks_for_roles(
            agent.roles(),
            Some(agent.namespace().clone()),
        )
        .await
        .map(|tasks| {
            tasks.into_iter().take(5).map(|t| PendingTaskDto {
                id: t.id().to_string(),
                title: t.title().to_string(),
                priority: t.priority().to_string(),
                assigned_roles: t.assigned_roles().to_vec(),
            }).collect()
        })
        .unwrap_or_default();

    let reviews = ReviewStore::find_pending_for_agent(&*container.store, &agent.id())
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

    Json(PendingWorkDto {
        messages,
        tasks,
        reviews,
    })
    .into_response()
}
