use orchy_application::{
    AddDependencyCommand, AssignTaskCommand, CancelTaskCommand, ClaimTaskCommand,
    CompleteTaskCommand, DelegateTaskCommand, FailTaskCommand, GetNextTaskCommand, GetTaskCommand,
    GetTaskWithContextCommand, ListTasksCommand, MergeTasksCommand, MoveTaskCommand,
    PostTaskCommand, ReleaseTaskCommand, RemoveDependencyCommand, ReplaceTaskCommand,
    SplitTaskCommand, StartTaskCommand, SubtaskInput, TagTaskCommand, UnblockTaskCommand,
    UntagTaskCommand, UpdateTaskCommand,
};

use crate::mcp::handler::{NamespacePolicy, OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{
    AddDependencyParams, AssignTaskParams, CancelTaskParams, ClaimTaskParams, CompleteTaskParams,
    DelegateTaskParams, FailTaskParams, GetNextTaskParams, GetTaskParams, ListTagsParams,
    ListTasksParams, MergeTasksParams, MoveTaskParams, PostTaskParams, ReleaseTaskParams,
    RemoveDependencyParams, ReplaceTaskParams, SplitTaskParams, StartTaskParams, TagTaskParams,
    UnblockTaskParams, UntagTaskParams, UpdateTaskParams,
};

use super::parse_relation_options;

use orchy_application::ListTagsCommand;

pub(super) async fn post_task(h: &OrchyHandler, params: PostTaskParams) -> Result<String, String> {
    let (_, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
        .await?;

    let cmd = PostTaskCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        title: params.title,
        description: params.description,
        acceptance_criteria: params.acceptance_criteria,
        priority: params.priority,
        assigned_roles: params.assigned_roles,
        created_by: h.get_session_agent().await.map(|id| id.to_string()),
    };

    match h.container.app.post_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn get_next_task(
    h: &OrchyHandler,
    params: GetNextTaskParams,
) -> Result<String, String> {
    let (agent_id, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let roles = match params.role {
        Some(r) => vec![r],
        None => match h
            .container
            .app
            .get_agent
            .execute(orchy_application::GetAgentCommand {
                agent_id: agent_id.to_string(),
                org_id: None,
                relations: None,
            })
            .await
        {
            Ok(agent) => agent.roles.clone(),
            Err(e) => return Err(format!("error fetching agent roles: {e}")),
        },
    };

    let claim = params.claim.unwrap_or(true);

    let cmd = GetNextTaskCommand {
        org_id: Some(org.to_string()),
        project: Some(project.to_string()),
        namespace: Some(namespace.to_string()),
        roles,
        claim: Some(claim),
        agent_id: if claim {
            Some(agent_id.to_string())
        } else {
            None
        },
    };

    match h.container.app.get_next_task.execute(cmd).await {
        Ok(Some(task)) => {
            let task_id = task
                .id
                .parse::<orchy_core::task::TaskId>()
                .map_err(|e| e.to_string())?;
            let ctx = h
                .container
                .app
                .get_task_with_context
                .execute(GetTaskWithContextCommand {
                    task_id: task_id.to_string(),
                    org_id: org.to_string(),
                    include_dependencies: false,
                    include_knowledge: false,
                    knowledge_limit: 20,
                    knowledge_kind: None,
                    knowledge_tag: None,
                    knowledge_content_limit: 500,
                })
                .await
                .map_err(mcp_error)?;
            Ok(to_json(&ctx))
        }
        Ok(None) => Ok(to_json(&serde_json::Value::Null)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn list_tasks(
    h: &OrchyHandler,
    params: ListTasksParams,
) -> Result<String, String> {
    let (_, org, session_project, _) = h.require_session().await?;

    let project = params
        .project
        .unwrap_or_else(|| session_project.to_string());

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let cmd = ListTasksCommand {
        org_id: org.to_string(),
        project: Some(project),
        namespace: Some(namespace.to_string()),
        status: params.status,
        assigned_to: None,
        tag: None,
        after: params.after,
        limit: params.limit,
    };

    match h.container.app.list_tasks.execute(cmd).await {
        Ok(page) => Ok(to_json(&page)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn claim_task(
    h: &OrchyHandler,
    params: ClaimTaskParams,
) -> Result<String, String> {
    let (agent_id, org, _, _) = h.require_session().await?;

    let cmd = ClaimTaskCommand {
        task_id: params.task_id.clone(),
        agent_id: agent_id.to_string(),
        org_id: org.to_string(),
        start: params.start,
    };

    match h.container.app.claim_task.execute(cmd).await {
        Ok(task) => {
            let task_id = task
                .id
                .parse::<orchy_core::task::TaskId>()
                .map_err(|e| e.to_string())?;
            let ctx = h
                .container
                .app
                .get_task_with_context
                .execute(GetTaskWithContextCommand {
                    task_id: task_id.to_string(),
                    org_id: org.to_string(),
                    include_dependencies: false,
                    include_knowledge: false,
                    knowledge_limit: 20,
                    knowledge_kind: None,
                    knowledge_tag: None,
                    knowledge_content_limit: 500,
                })
                .await
                .map_err(mcp_error)?;
            Ok(to_json(&ctx))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn start_task(
    h: &OrchyHandler,
    params: StartTaskParams,
) -> Result<String, String> {
    let (agent_id, org, _, _) = h.require_session().await?;

    let cmd = StartTaskCommand {
        task_id: params.task_id.clone(),
        agent_id: agent_id.to_string(),
    };

    match h.container.app.start_task.execute(cmd).await {
        Ok(task) => {
            let task_id = task
                .id
                .parse::<orchy_core::task::TaskId>()
                .map_err(|e| e.to_string())?;
            let ctx = h
                .container
                .app
                .get_task_with_context
                .execute(GetTaskWithContextCommand {
                    task_id: task_id.to_string(),
                    org_id: org.to_string(),
                    include_dependencies: false,
                    include_knowledge: false,
                    knowledge_limit: 20,
                    knowledge_kind: None,
                    knowledge_tag: None,
                    knowledge_content_limit: 500,
                })
                .await
                .map_err(mcp_error)?;
            Ok(to_json(&ctx))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn complete_task(
    h: &OrchyHandler,
    params: CompleteTaskParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;
    let cmd = CompleteTaskCommand {
        task_id: params.task_id,
        org_id: org.to_string(),
        summary: params.summary,
        links: params
            .links
            .unwrap_or_default()
            .into_iter()
            .map(|l| {
                let rel_type = super::parse_rel_type_alias(&l.rel_type)
                    .map(|r| r.to_string())
                    .unwrap_or(l.rel_type);
                orchy_core::graph::neighborhood::LinkParam {
                    to_kind: l.to_kind,
                    to_id: l.to_id,
                    rel_type,
                }
            })
            .collect(),
    };

    match h.container.app.complete_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn fail_task(h: &OrchyHandler, params: FailTaskParams) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let cmd = FailTaskCommand {
        task_id: params.task_id,
        org_id: org.to_string(),
        reason: params.reason,
    };

    match h.container.app.fail_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn cancel_task(
    h: &OrchyHandler,
    params: CancelTaskParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let cmd = CancelTaskCommand {
        task_id: params.task_id,
        org_id: org.to_string(),
        reason: params.reason,
    };

    match h.container.app.cancel_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn update_task(
    h: &OrchyHandler,
    params: UpdateTaskParams,
) -> Result<String, String> {
    let _ = h.require_session().await?;

    let cmd = UpdateTaskCommand {
        task_id: params.task_id,
        title: params.title,
        description: params.description,
        acceptance_criteria: params.acceptance_criteria,
        priority: params.priority,
    };

    match h.container.app.update_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn unblock_task(
    h: &OrchyHandler,
    params: UnblockTaskParams,
) -> Result<String, String> {
    let _ = h.require_session().await?;

    let cmd = UnblockTaskCommand {
        task_id: params.task_id,
    };

    match h.container.app.unblock_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn assign_task(
    h: &OrchyHandler,
    params: AssignTaskParams,
) -> Result<String, String> {
    let _ = h.require_session().await?;

    let agent_id = h.resolve_agent_id(&params.agent).await?;

    let cmd = AssignTaskCommand {
        task_id: params.task_id,
        agent_id: agent_id.to_string(),
    };

    match h.container.app.assign_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn split_task(
    h: &OrchyHandler,
    params: SplitTaskParams,
) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;

    let subtasks = params
        .subtasks
        .into_iter()
        .map(|sp| SubtaskInput {
            title: sp.title,
            description: sp.description,
            acceptance_criteria: sp.acceptance_criteria,
            priority: sp.priority,
            assigned_roles: sp.assigned_roles,
            depends_on: sp.depends_on,
        })
        .collect();

    let cmd = SplitTaskCommand {
        task_id: params.task_id,
        subtasks,
        created_by: Some(agent_id.to_string()),
    };

    match h.container.app.split_task.execute(cmd).await {
        Ok((parent, children)) => {
            let result = serde_json::json!({
                "parent": parent,
                "subtasks": children,
            });
            Ok(to_json(&result))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn replace_task(
    h: &OrchyHandler,
    params: ReplaceTaskParams,
) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;

    let replacements = params
        .replacements
        .into_iter()
        .map(|sp| SubtaskInput {
            title: sp.title,
            description: sp.description,
            acceptance_criteria: sp.acceptance_criteria,
            priority: sp.priority,
            assigned_roles: sp.assigned_roles,
            depends_on: sp.depends_on,
        })
        .collect();

    let cmd = ReplaceTaskCommand {
        task_id: params.task_id,
        reason: params.reason,
        replacements,
        created_by: Some(agent_id.to_string()),
    };

    match h.container.app.replace_task.execute(cmd).await {
        Ok((original, new_tasks)) => {
            let result = serde_json::json!({
                "cancelled": original,
                "replacements": new_tasks,
            });
            Ok(to_json(&result))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn merge_tasks(
    h: &OrchyHandler,
    params: MergeTasksParams,
) -> Result<String, String> {
    let (agent_id, org, _, _) = h.require_session().await?;

    let cmd = MergeTasksCommand {
        org_id: org.to_string(),
        task_ids: params.task_ids,
        title: params.title,
        description: params.description,
        acceptance_criteria: params.acceptance_criteria,
        created_by: Some(agent_id.to_string()),
    };

    match h.container.app.merge_tasks.execute(cmd).await {
        Ok((merged, cancelled)) => {
            let result = serde_json::json!({
                "merged": merged,
                "cancelled": cancelled,
            });
            Ok(to_json(&result))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn delegate_task(
    h: &OrchyHandler,
    params: DelegateTaskParams,
) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;

    let cmd = DelegateTaskCommand {
        task_id: params.task_id,
        title: params.title,
        description: params.description,
        acceptance_criteria: params.acceptance_criteria,
        priority: params.priority,
        assigned_roles: params.assigned_roles,
        created_by: Some(agent_id.to_string()),
    };

    match h.container.app.delegate_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn add_dependency(
    h: &OrchyHandler,
    params: AddDependencyParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let cmd = AddDependencyCommand {
        org_id: org.to_string(),
        task_id: params.task_id,
        dependency_id: params.dependency_id,
    };

    match h.container.app.add_dependency.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn remove_dependency(
    h: &OrchyHandler,
    params: RemoveDependencyParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let cmd = RemoveDependencyCommand {
        org_id: org.to_string(),
        task_id: params.task_id,
        dependency_id: params.dependency_id,
    };

    match h.container.app.remove_dependency.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn move_task(h: &OrchyHandler, params: MoveTaskParams) -> Result<String, String> {
    let _ = h.require_session().await?;

    let namespace = h
        .resolve_namespace(Some(&params.new_namespace), NamespacePolicy::RegisterIfNew)
        .await?;

    let cmd = MoveTaskCommand {
        task_id: params.task_id,
        new_namespace: namespace.to_string(),
    };

    match h.container.app.move_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn tag_task(h: &OrchyHandler, params: TagTaskParams) -> Result<String, String> {
    let _ = h.require_session().await?;

    let cmd = TagTaskCommand {
        task_id: params.task_id,
        tag: params.tag,
    };

    match h.container.app.tag_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn untag_task(
    h: &OrchyHandler,
    params: UntagTaskParams,
) -> Result<String, String> {
    let _ = h.require_session().await?;

    let cmd = UntagTaskCommand {
        task_id: params.task_id,
        tag: params.tag,
    };

    match h.container.app.untag_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn release_task(
    h: &OrchyHandler,
    params: ReleaseTaskParams,
) -> Result<String, String> {
    let _ = h.require_session().await?;

    let cmd = ReleaseTaskCommand {
        task_id: params.task_id,
    };

    match h.container.app.release_task.execute(cmd).await {
        Ok(task) => Ok(to_json(&task)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn get_task(h: &OrchyHandler, params: GetTaskParams) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let relations = parse_relation_options(params.relations);
    if relations.is_some()
        || (!params.include_dependencies.unwrap_or(false)
            && !params.include_knowledge.unwrap_or(false))
    {
        match h
            .container
            .app
            .get_task
            .execute(GetTaskCommand {
                task_id: params.task_id.clone(),
                org_id: Some(org.to_string()),
                relations,
            })
            .await
        {
            Ok(resp) => return Ok(to_json(&resp)),
            Err(e) => return Err(mcp_error(e)),
        }
    }

    match h
        .container
        .app
        .get_task_with_context
        .execute(GetTaskWithContextCommand {
            task_id: params.task_id.clone(),
            org_id: org.to_string(),
            include_dependencies: params.include_dependencies.unwrap_or(false),
            include_knowledge: params.include_knowledge.unwrap_or(false),
            knowledge_limit: params.knowledge_limit.unwrap_or(20),
            knowledge_kind: params.knowledge_kind,
            knowledge_tag: params.knowledge_tag,
            knowledge_content_limit: params.knowledge_content_limit.unwrap_or(500),
        })
        .await
    {
        Ok(ctx) => Ok(to_json(&ctx)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn list_tags(h: &OrchyHandler, params: ListTagsParams) -> Result<String, String> {
    let (_, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let cmd = ListTagsCommand {
        org_id: Some(org.to_string()),
        project: Some(project.to_string()),
        namespace: Some(namespace.to_string()),
    };

    match h.container.app.list_tags.execute(cmd).await {
        Ok(tags) => Ok(to_json(&tags)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn touch_task(h: &OrchyHandler, task_id: String) -> Result<String, String> {
    let (agent_id, _org, _project, _ns) = h.require_session().await?;
    let cmd = orchy_application::TouchTaskCommand {
        task_id,
        agent_id: Some(agent_id.to_string()),
    };
    match h.container.app.touch_task.execute(cmd).await {
        Ok(response) => Ok(to_json(&response)),
        Err(e) => Err(mcp_error(e)),
    }
}
