use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_core::agent::{AgentId, AgentStatus};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{Priority, SubtaskDef, Task, TaskFilter, TaskId, TaskStatus};

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

fn parse_task_id(s: &str) -> Result<TaskId, ApiError> {
    s.parse::<TaskId>().map_err(|e| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            format!("invalid task id: {e}"),
        )
    })
}

async fn resolve_agent(
    container: &Arc<Container>,
    org_id: &OrganizationId,
    project_id: &ProjectId,
    s: &str,
) -> Result<AgentId, ApiError> {
    let agent_id = s.parse::<AgentId>().map_err(|e| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            format!("invalid agent id: {e}"),
        )
    })?;

    let agent = container
        .agent_service
        .get(&agent_id)
        .await
        .map_err(ApiError::from)?;

    if agent.org_id() != org_id
        || agent.project() != project_id
        || agent.status() == AgentStatus::Disconnected
    {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("agent not found: {s}"),
        ));
    }

    Ok(agent_id)
}

fn parse_ns(ns: Option<&str>) -> Result<Option<Namespace>, ApiError> {
    match ns {
        Some(s) => parse_namespace(s).map(Some),
        None => Ok(None),
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

fn check_task_project(task: &Task, project_id: &ProjectId) -> Result<(), ApiError> {
    if task.project() != project_id {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("task {} not found in project {project_id}", task.id()),
        ));
    }
    Ok(())
}

type OrgProject = (String, String);

#[derive(Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    pub namespace: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Deserialize)]
pub struct NextTaskQuery {
    pub namespace: Option<String>,
    pub role: Option<String>,
    pub claim: Option<bool>,
}

#[derive(Deserialize)]
pub struct NamespaceQuery {
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct PostTaskBody {
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub parent_id: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTaskBody {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
}

#[derive(Deserialize)]
pub struct ClaimBody {
    pub agent: String,
    pub start: Option<bool>,
}

#[derive(Deserialize)]
pub struct AgentBody {
    pub agent: String,
}

#[derive(Deserialize)]
pub struct CompleteBody {
    pub summary: Option<String>,
}

#[derive(Deserialize)]
pub struct FailBody {
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct CancelBody {
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct AddNoteBody {
    pub agent: Option<String>,
    pub body: String,
}

#[derive(Deserialize)]
pub struct AddDepBody {
    pub dependency_id: String,
}

#[derive(Deserialize)]
pub struct UnwatchQuery {
    pub agent: String,
}

#[derive(Deserialize)]
pub struct AgentQuery {
    pub agent: String,
}

#[derive(Deserialize)]
pub struct SubtaskDefBody {
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct SplitBody {
    pub subtasks: Vec<SubtaskDefBody>,
}

#[derive(Deserialize)]
pub struct ReplaceBody {
    pub reason: Option<String>,
    pub replacements: Vec<SubtaskDefBody>,
}

#[derive(Deserialize)]
pub struct MergeBody {
    pub task_ids: Vec<String>,
    pub title: String,
    pub description: String,
}

#[derive(Deserialize)]
pub struct DelegateBody {
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
}

fn parse_subtask_defs(defs: Vec<SubtaskDefBody>) -> Result<Vec<SubtaskDef>, ApiError> {
    defs.into_iter()
        .map(|d| {
            let priority = match d.priority.as_deref() {
                Some(p) => p.parse::<Priority>().map_err(|e| {
                    ApiError(
                        StatusCode::BAD_REQUEST,
                        "INVALID_PARAM",
                        format!("invalid priority: {e}"),
                    )
                })?,
                None => Priority::default(),
            };
            let depends_on = d
                .depends_on
                .unwrap_or_default()
                .iter()
                .map(|s| parse_task_id(s))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SubtaskDef {
                title: d.title,
                description: d.description,
                priority,
                assigned_roles: d.assigned_roles.unwrap_or_default(),
                depends_on,
            })
        })
        .collect()
}

pub async fn list(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;

    let ns = parse_ns(query.namespace.as_deref())?;

    let status = match query.status.as_deref() {
        Some("pending") => Some(TaskStatus::Pending),
        Some("blocked") => Some(TaskStatus::Blocked),
        Some("claimed") => Some(TaskStatus::Claimed),
        Some("in_progress") => Some(TaskStatus::InProgress),
        Some("completed") => Some(TaskStatus::Completed),
        Some("failed") => Some(TaskStatus::Failed),
        Some("cancelled") => Some(TaskStatus::Cancelled),
        Some(other) => {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "INVALID_PARAM",
                format!("invalid status: {other}"),
            ));
        }
        None => None,
    };

    let parent_id = query.parent_id.as_deref().map(parse_task_id).transpose()?;

    let filter = TaskFilter {
        project: Some(project_id),
        namespace: ns,
        status,
        parent_id,
        ..Default::default()
    };

    let page = container
        .task_service
        .list(filter, orchy_core::pagination::PageParams::unbounded())
        .await
        .map_err(ApiError::from)?;

    Ok(Json(page.items))
}

pub async fn post(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Json(body): Json<PostTaskBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;

    let ns = match body.namespace.as_deref() {
        Some(s) => parse_namespace(s)?,
        None => Namespace::root(),
    };

    let priority = match body.priority.as_deref() {
        Some(p) => p.parse::<Priority>().map_err(|e| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "INVALID_PARAM",
                format!("invalid priority: {e}"),
            )
        })?,
        None => Priority::default(),
    };

    let depends_on = body
        .depends_on
        .unwrap_or_default()
        .iter()
        .map(|s| parse_task_id(s))
        .collect::<Result<Vec<_>, _>>()?;

    let parent_id = body.parent_id.as_deref().map(parse_task_id).transpose()?;

    let is_blocked = !depends_on.is_empty();

    let task = Task::new(
        org_id,
        project_id,
        ns,
        parent_id,
        body.title,
        body.description,
        priority,
        body.assigned_roles.unwrap_or_default(),
        depends_on,
        None,
        is_blocked,
    )
    .map_err(|e| {
        ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "INVALID_INPUT",
            e.to_string(),
        )
    })?;

    let task_id = task.id();
    container
        .task_service
        .create(task)
        .await
        .map_err(ApiError::from)?;

    let created = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(created))
}

pub async fn get_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
) -> Result<Json<orchy_core::task::TaskWithContext>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;
    let ctx = container
        .task_service
        .get_with_context(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&ctx.task, &project_id)?;
    Ok(Json(ctx))
}

pub async fn update_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<UpdateTaskBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let priority = match body.priority.as_deref() {
        Some(p) => Some(p.parse::<Priority>().map_err(|e| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "INVALID_PARAM",
                format!("invalid priority: {e}"),
            )
        })?),
        None => None,
    };

    let task = container
        .task_service
        .update_details(&task_id, body.title, body.description, priority)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(task))
}

pub async fn claim(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<ClaimBody>,
) -> Result<Json<orchy_core::task::TaskWithContext>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;
    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let agent_id = resolve_agent(&container, &org_id, &project_id, &body.agent).await?;

    container
        .task_service
        .claim(&task_id, &agent_id)
        .await
        .map_err(ApiError::from)?;

    if body.start.unwrap_or(false) {
        container
            .task_service
            .start(&task_id, &agent_id)
            .await
            .map_err(ApiError::from)?;
    }

    let ctx = container
        .task_service
        .get_with_context(&task_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ctx))
}

pub async fn start(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AgentBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let agent_id = resolve_agent(&container, &org_id, &project_id, &body.agent).await?;
    let task = container
        .task_service
        .start(&task_id, &agent_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn complete(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<CompleteBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .complete(&task_id, body.summary)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn fail(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<FailBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .fail(&task_id, body.reason)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn cancel(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<CancelBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .cancel(&task_id, body.reason)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn release(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .release(&task_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn unblock(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .unblock_manual(&task_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn assign(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AgentBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let agent_id = resolve_agent(&container, &org_id, &project_id, &body.agent).await?;
    let task = container
        .task_service
        .assign(&task_id, &agent_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn watch(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AgentBody>,
) -> Result<Json<orchy_core::task::TaskWatcher>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let agent_id = resolve_agent(&container, &org_id, &project_id, &body.agent).await?;

    let watcher = container
        .task_service
        .watch(
            &task_id,
            agent_id,
            org_id,
            project_id,
            existing.namespace().clone(),
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(watcher))
}

pub async fn unwatch(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Query(query): Query<UnwatchQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let agent_id = resolve_agent(&container, &org_id, &project_id, &query.agent).await?;

    container
        .task_service
        .unwatch(&task_id, &agent_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn add_note(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AddNoteBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let agent_id = if let Some(s) = body.agent.as_deref() {
        Some(resolve_agent(&container, &org_id, &project_id, s).await?)
    } else {
        None
    };

    let task = container
        .task_service
        .add_note(&task_id, agent_id, body.body)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(task))
}

pub async fn add_dep(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AddDepBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let dep_id = parse_task_id(&body.dependency_id)?;
    let task = container
        .task_service
        .add_dependency(&task_id, &dep_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn remove_dep(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id, dep_id)): Path<(String, String, String, String)>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let dep_id = parse_task_id(&dep_id)?;
    let task = container
        .task_service
        .remove_dependency(&task_id, &dep_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn tag_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id, tag)): Path<(String, String, String, String)>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .tag(&task_id, tag)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn untag_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id, tag)): Path<(String, String, String, String)>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;

    let task = container
        .task_service
        .untag(&task_id, &tag)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(task))
}

pub async fn list_tags(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<Vec<String>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let page = container
        .task_service
        .list(
            TaskFilter {
                project: Some(project_id),
                namespace: ns,
                ..Default::default()
            },
            orchy_core::pagination::PageParams::unbounded(),
        )
        .await
        .map_err(ApiError::from)?;

    let mut tags: Vec<String> = page
        .items
        .iter()
        .flat_map(|t| t.tags().iter().cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    tags.sort();

    Ok(Json(tags))
}

pub async fn next_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project)): Path<OrgProject>,
    Query(query): Query<NextTaskQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let ns = parse_ns(query.namespace.as_deref())?;
    let roles = match query.role {
        Some(r) => vec![r],
        None => vec![],
    };

    let claim = query.claim.unwrap_or(false);

    if roles.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            "role query param required".to_string(),
        ));
    }

    if claim {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            "claim requires agent; use POST /tasks/:id/claim".to_string(),
        ));
    }

    let task = container
        .task_service
        .peek_next(&roles, ns)
        .await
        .map_err(ApiError::from)?;

    match task {
        Some(t) => {
            let ctx = container
                .task_service
                .get_with_context(&t.id())
                .await
                .map_err(ApiError::from)?;
            Ok(Json(serde_json::to_value(ctx).unwrap()))
        }
        None => Ok(Json(serde_json::Value::Null)),
    }
}

pub async fn split(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<SplitBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;
    let subtasks = parse_subtask_defs(body.subtasks)?;

    let (parent, children) = container
        .task_service
        .split_task(&task_id, subtasks, None)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        serde_json::json!({"parent": parent, "subtasks": children}),
    ))
}

pub async fn replace(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<ReplaceBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&id)?;

    let existing = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project_id)?;
    let replacements = parse_subtask_defs(body.replacements)?;

    let (original, new_tasks) = container
        .task_service
        .replace_task(&task_id, body.reason, replacements, None)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        serde_json::json!({"cancelled": original, "replacements": new_tasks}),
    ))
}

pub async fn merge(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Json(body): Json<MergeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;

    let task_ids = body
        .task_ids
        .iter()
        .map(|s| parse_task_id(s))
        .collect::<Result<Vec<_>, _>>()?;

    for tid in &task_ids {
        let existing = container
            .task_service
            .get(tid)
            .await
            .map_err(ApiError::from)?;
        check_task_project(&existing, &project_id)?;
    }

    let (merged, cancelled) = container
        .task_service
        .merge_tasks(&task_ids, body.title, body.description, None)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(
        serde_json::json!({"merged": merged, "cancelled": cancelled}),
    ))
}

pub async fn delegate(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<DelegateBody>,
) -> Result<Json<Task>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let parent_id = parse_task_id(&id)?;

    let parent = container
        .task_service
        .get(&parent_id)
        .await
        .map_err(ApiError::from)?;
    check_task_project(&parent, &project_id)?;

    let priority = match body.priority.as_deref() {
        Some(p) => p.parse::<Priority>().map_err(|e| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "INVALID_PARAM",
                format!("invalid priority: {e}"),
            )
        })?,
        None => parent.priority(),
    };

    let task = Task::new(
        org_id,
        project_id,
        parent.namespace().clone(),
        Some(parent_id),
        body.title,
        body.description,
        priority,
        body.assigned_roles.unwrap_or_default(),
        vec![],
        None,
        false,
    )
    .map_err(|e| {
        ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "INVALID_INPUT",
            e.to_string(),
        )
    })?;

    let task_id = task.id();
    container
        .task_service
        .create(task)
        .await
        .map_err(ApiError::from)?;

    let created = container
        .task_service
        .get(&task_id)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(created))
}
