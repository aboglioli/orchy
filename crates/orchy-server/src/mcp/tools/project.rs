use orchy_application::{
    CheckLockCommand, GetProjectCommand, ListAgentsCommand, ListNamespacesCommand,
    ListTasksCommand, LockResourceCommand, SetProjectMetadataCommand, UnlockResourceCommand,
    UpdateProjectCommand,
};

use crate::mcp::handler::{NamespacePolicy, OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{
    CheckLockParams, GetProjectParams, ListNamespacesParams, LockResourceParams,
    SetProjectMetadataParams, UnlockResourceParams, UpdateProjectParams,
};

pub(super) async fn get_project(
    h: &OrchyHandler,
    params: GetProjectParams,
) -> Result<String, String> {
    let (_, org, project_id, _) = h.require_session().await?;

    let cmd = GetProjectCommand {
        org_id: org.to_string(),
        project: project_id.to_string(),
    };

    let project = h
        .container
        .app
        .get_project
        .execute(cmd)
        .await
        .map_err(mcp_error)?;

    if !params.include_summary.unwrap_or(false) {
        return Ok(to_json(&project));
    }

    let agents_cmd = ListAgentsCommand {
        org_id: org.to_string(),
        project: Some(project_id.to_string()),
        after: None,
        limit: None,
    };
    let project_agents: Vec<_> = h
        .container
        .app
        .list_agents
        .execute(agents_cmd)
        .await
        .map_err(mcp_error)?
        .items
        .into_iter()
        // Status is derived (active/idle/stale); all agents are included
        .collect();

    let tasks_cmd = ListTasksCommand {
        org_id: org.to_string(),
        project: Some(project_id.to_string()),
        namespace: None,
        status: None,
        assigned_to: None,
        tag: None,
        after: None,
        limit: None,
    };
    let all_tasks = h
        .container
        .app
        .list_tasks
        .execute(tasks_cmd)
        .await
        .map_err(mcp_error)?
        .items;

    let mut by_status = std::collections::HashMap::new();
    for task in &all_tasks {
        *by_status.entry(task.status.clone()).or_insert(0u32) += 1;
    }

    let mut recent: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.status == "completed" || t.status == "failed")
        .collect();
    recent.sort_by_key(|b| std::cmp::Reverse(&b.updated_at));
    recent.truncate(10);

    let recent_items: Vec<_> = recent
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": &t.id,
                "title": &t.title,
                "status": &t.status,
                "summary": &t.result_summary,
            })
        })
        .collect();

    let agent_id = h.get_session_agent().await;
    let mut my_workload_by_status: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    if let Some(ref aid) = agent_id {
        let aid_str = aid.to_string();
        for task in &all_tasks {
            if task.assigned_to.as_deref() == Some(aid_str.as_str()) {
                my_workload_by_status
                    .entry(task.status.clone())
                    .or_default()
                    .push(serde_json::json!({
                        "id": &task.id,
                        "title": &task.title,
                        "priority": &task.priority,
                    }));
            }
        }
    }

    let my_task_count: usize = my_workload_by_status.values().map(|v| v.len()).sum();

    let summary = serde_json::json!({
        "agents_online": project_agents.len(),
        "tasks_by_status": by_status,
        "total_tasks": all_tasks.len(),
        "recent_completions": recent_items,
        "my_workload": {
            "total_tasks": my_task_count,
            "by_status": my_workload_by_status,
        },
    });

    Ok(to_json(&serde_json::json!({
        "project": project,
        "summary": summary,
    })))
}

pub(super) async fn update_project(
    h: &OrchyHandler,
    params: UpdateProjectParams,
) -> Result<String, String> {
    let (_, org, project_id, _) = h.require_session().await?;

    let project_cmd = GetProjectCommand {
        org_id: org.to_string(),
        project: project_id.to_string(),
    };
    let project = h
        .container
        .app
        .get_project
        .execute(project_cmd)
        .await
        .map_err(mcp_error)?;

    if let Some(expected) = params.version {
        let updated = chrono::DateTime::parse_from_rfc3339(&project.updated_at)
            .map(|dt| dt.timestamp() as u64)
            .unwrap_or(0);
        if expected != updated {
            return Err(format!(
                "version mismatch: expected {}, got {}",
                expected, updated
            ));
        }
    }

    let description = params
        .description
        .unwrap_or_else(|| project.description.clone());

    let cmd = UpdateProjectCommand {
        org_id: org.to_string(),
        project: project_id.to_string(),
        description,
    };

    match h.container.app.update_project.execute(cmd).await {
        Ok(project) => Ok(to_json(&project)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn set_project_metadata(
    h: &OrchyHandler,
    params: SetProjectMetadataParams,
) -> Result<String, String> {
    let (_, org, project_id, _) = h.require_session().await?;

    let cmd = SetProjectMetadataCommand {
        org_id: org.to_string(),
        project: project_id.to_string(),
        key: params.key,
        value: params.value,
    };

    match h.container.app.set_project_metadata.execute(cmd).await {
        Ok(project) => Ok(to_json(&project)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn list_namespaces(
    h: &OrchyHandler,
    params: ListNamespacesParams,
) -> Result<String, String> {
    let (_, org, session_project, _) = h.require_session().await?;
    let project = if let Some(p) = params.project {
        p
    } else {
        session_project.to_string()
    };

    let cmd = ListNamespacesCommand {
        org_id: org.to_string(),
        project,
    };

    match h.container.app.list_namespaces.execute(cmd).await {
        Ok(namespaces) => Ok(to_json(&namespaces)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn lock_resource(
    h: &OrchyHandler,
    params: LockResourceParams,
) -> Result<String, String> {
    let (agent_id, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
        .await?;

    let cmd = LockResourceCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        name: params.name,
        holder_agent_id: agent_id.to_string(),
        ttl_secs: params.ttl_secs,
    };

    match h.container.app.lock_resource.execute(cmd).await {
        Ok(lock) => Ok(to_json(&lock)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn unlock_resource(
    h: &OrchyHandler,
    params: UnlockResourceParams,
) -> Result<String, String> {
    let (agent_id, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = UnlockResourceCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        name: params.name,
        holder_agent_id: agent_id.to_string(),
    };

    match h.container.app.unlock_resource.execute(cmd).await {
        Ok(()) => Ok(r#"{"ok":true}"#.to_string()),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn check_lock(
    h: &OrchyHandler,
    params: CheckLockParams,
) -> Result<String, String> {
    let (_, org, project, _) = h.require_session().await?;

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = CheckLockCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        name: params.name,
    };

    match h.container.app.check_lock.execute(cmd).await {
        Ok(Some(lock)) => Ok(to_json(&lock)),
        Ok(None) => Ok(to_json(&serde_json::Value::Null)),
        Err(e) => Err(mcp_error(e)),
    }
}
