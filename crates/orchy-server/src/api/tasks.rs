use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

use orchy_application::{
    AddDependencyCommand, AssignTaskCommand, CancelTaskCommand, ClaimTaskCommand,
    CompleteTaskCommand, DelegateTaskCommand, FailTaskCommand, GetNextTaskCommand, ListTagsCommand,
    ListTasksCommand, MergeTasksCommand, PostTaskCommand, ReleaseTaskCommand,
    RemoveDependencyCommand, ReplaceTaskCommand, SplitTaskCommand, StartTaskCommand, SubtaskInput,
    TagTaskCommand, UnblockTaskCommand, UntagTaskCommand, UpdateTaskCommand,
};
use orchy_application::{GetTaskCommand, TaskResponse};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

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

fn check_task_project(task: &TaskResponse, project: &str) -> Result<(), ApiError> {
    if task.project != project {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("task {} not found in project {project}", task.id),
        ));
    }
    Ok(())
}

use super::ApiError;

type OrgProject = (String, String);

#[derive(Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    pub namespace: Option<String>,
    pub parent_id: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
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
    pub acceptance_criteria: Option<String>,
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
    pub acceptance_criteria: Option<String>,
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
pub struct AddDepBody {
    pub dependency_id: String,
}

#[derive(Deserialize)]
pub struct AgentQuery {
    pub agent: String,
}

#[derive(Deserialize)]
pub struct SubtaskDefBody {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
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
    pub acceptance_criteria: Option<String>,
}

#[derive(Deserialize)]
pub struct DelegateBody {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
}

fn to_subtask_inputs(defs: Vec<SubtaskDefBody>) -> Vec<SubtaskInput> {
    defs.into_iter()
        .map(|d| SubtaskInput {
            title: d.title,
            description: d.description,
            acceptance_criteria: d.acceptance_criteria,
            priority: d.priority,
            assigned_roles: d.assigned_roles,
            depends_on: d.depends_on,
        })
        .collect()
}

pub async fn list(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ListTasksCommand {
        org_id: org,
        project: Some(project),
        namespace: query.namespace,
        status: query.status,
        parent_id: query.parent_id,
        assigned_to: None,
        tag: None,
        after: query.after,
        limit: query.limit,
    };

    let page = container
        .app
        .list_tasks
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&page).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn post(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Json(body): Json<PostTaskBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = PostTaskCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        title: body.title,
        description: body.description,
        acceptance_criteria: body.acceptance_criteria,
        priority: body.priority,
        assigned_roles: body.assigned_roles,
        depends_on: body.depends_on,
        parent_id: body.parent_id,
        created_by: None,
    };

    let task = container
        .app
        .post_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn get_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let task = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&task, &project)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn update_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<UpdateTaskBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = UpdateTaskCommand {
        task_id: id,
        title: body.title,
        description: body.description,
        acceptance_criteria: body.acceptance_criteria,
        priority: body.priority,
    };

    let task = container
        .app
        .update_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn claim(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<ClaimBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = ClaimTaskCommand {
        task_id: id,
        agent_id: body.agent,
        start: body.start,
    };

    let task = container
        .app
        .claim_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn start(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = StartTaskCommand {
        task_id: id,
        agent_id: body.agent,
    };

    let task = container
        .app
        .start_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn complete(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<CompleteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = CompleteTaskCommand {
        task_id: id,
        org_id: org,
        summary: body.summary,
        links: vec![],
    };

    let task = container
        .app
        .complete_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn fail(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<FailBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = FailTaskCommand {
        task_id: id,
        reason: body.reason,
    };

    let task = container
        .app
        .fail_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn cancel(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<CancelBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = CancelTaskCommand {
        task_id: id,
        reason: body.reason,
    };

    let task = container
        .app
        .cancel_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn release(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = ReleaseTaskCommand { task_id: id };

    let task = container
        .app
        .release_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn unblock(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = UnblockTaskCommand { task_id: id };

    let task = container
        .app
        .unblock_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn assign(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AgentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = AssignTaskCommand {
        task_id: id,
        agent_id: body.agent,
    };

    let task = container
        .app
        .assign_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn add_dep(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<AddDepBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = AddDependencyCommand {
        org_id: org_id.to_string(),
        task_id: id,
        dependency_id: body.dependency_id,
    };

    let task = container
        .app
        .add_dependency
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn remove_dep(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id, dep_id)): Path<(String, String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = RemoveDependencyCommand {
        org_id: org_id.to_string(),
        task_id: id,
        dependency_id: dep_id,
    };

    let task = container
        .app
        .remove_dependency
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn tag_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id, tag)): Path<(String, String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = TagTaskCommand { task_id: id, tag };

    let task = container
        .app
        .tag_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn untag_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id, tag)): Path<(String, String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = UntagTaskCommand { task_id: id, tag };

    let task = container
        .app
        .untag_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn list_tags(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ListTagsCommand {
        org_id: Some(org),
        project: Some(project),
        namespace: query.namespace,
    };

    let tags = container
        .app
        .list_tags
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&tags).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn next_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<OrgProject>,
    Query(query): Query<NextTaskQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let roles = match query.role {
        Some(r) => vec![r],
        None => vec![],
    };

    if roles.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            "role query param required".to_string(),
        ));
    }

    let claim = query.claim.unwrap_or(false);
    if claim {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "INVALID_PARAM",
            "claim requires agent; use POST /tasks/:id/claim".to_string(),
        ));
    }

    let cmd = GetNextTaskCommand {
        org_id: Some(org),
        project: Some(project),
        namespace: query.namespace,
        roles,
        claim: Some(false),
        agent_id: None,
    };

    let task = container
        .app
        .get_next_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}

pub async fn split(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, id)): Path<(String, String, String)>,
    Json(body): Json<SplitBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = SplitTaskCommand {
        task_id: id,
        subtasks: to_subtask_inputs(body.subtasks),
        created_by: None,
    };

    let (parent, children) = container
        .app
        .split_task
        .execute(cmd)
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

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = ReplaceTaskCommand {
        task_id: id,
        reason: body.reason,
        replacements: to_subtask_inputs(body.replacements),
        created_by: None,
    };

    let (original, new_tasks) = container
        .app
        .replace_task
        .execute(cmd)
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

    for tid in &body.task_ids {
        let existing = container
            .app
            .get_task
            .execute(GetTaskCommand {
                task_id: tid.clone(),
                org_id: None,
                relations: None,
            })
            .await
            .map_err(ApiError::from)?;
        check_task_project(&existing, &project)?;
    }

    let cmd = MergeTasksCommand {
        org_id: org,
        task_ids: body.task_ids,
        title: body.title,
        description: body.description,
        acceptance_criteria: body.acceptance_criteria,
        created_by: None,
    };

    let (merged, cancelled) = container
        .app
        .merge_tasks
        .execute(cmd)
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
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let existing = container
        .app
        .get_task
        .execute(GetTaskCommand {
            task_id: id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .map_err(ApiError::from)?;
    check_task_project(&existing, &project)?;

    let cmd = DelegateTaskCommand {
        task_id: id,
        title: body.title,
        description: body.description,
        acceptance_criteria: body.acceptance_criteria,
        priority: body.priority,
        assigned_roles: body.assigned_roles,
        created_by: None,
    };

    let task = container
        .app
        .delegate_task
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&task).map_err(|e| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZATION_ERROR",
            e.to_string(),
        )
    })?))
}
