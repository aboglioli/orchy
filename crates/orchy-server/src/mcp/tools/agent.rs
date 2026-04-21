use std::str::FromStr;

use orchy_application::{
    ChangeRolesCommand, CheckMailboxCommand, CheckSentMessagesCommand,
    GetAgentCommand, GetAgentSummaryCommand, HeartbeatCommand, ListAgentsCommand,
    ListConversationCommand, MarkReadCommand, PollUpdatesCommand, RegisterAgentCommand,
    RenameAliasCommand, SwitchContextCommand,
};

use crate::mcp::handler::{
    NamespacePolicy, OrchyHandler, default_org, mcp_error, parse_project, to_json,
};
use crate::mcp::params::{
    ChangeRolesParams, CheckMailboxParams, CheckSentMessagesParams, GetAgentContextParams,
    ListAgentsParams, ListConversationParams, MarkReadParams, PollUpdatesParams,
    RegisterAgentParams, SwitchContextParams,
};

use super::parse_relation_options;

pub(super) async fn register_agent(
    h: &OrchyHandler,
    params: RegisterAgentParams,
) -> Result<String, String> {
    if params.alias.is_empty() {
        return Err(
            "alias is required: call register_agent with alias='<name>' (lowercase alphanumeric with hyphens, e.g. 'coder-1')"
                .to_string(),
        );
    }
    let project = parse_project(&params.project)?;

    let namespace = match params.namespace.as_deref() {
        Some(s) if !s.is_empty() => Some(format!("/{s}")),
        _ => None,
    };

    let org_id = match params.organization.as_deref() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => default_org().to_string(),
    };

    let _ = h
        .resolve_namespace_for(
            namespace.as_deref(),
            NamespacePolicy::RegisterIfNew,
            None,
            None,
        )
        .await;

    let input_roles = params.roles.unwrap_or_default();
    let roles = if input_roles.is_empty() {
        let cmd = orchy_application::SuggestRolesCommand {
            org_id: Some(org_id.clone()),
            project: params.project.clone(),
            namespace: namespace.clone(),
        };
        match h.container.app.suggest_roles.execute(cmd).await {
            Ok(r) if !r.is_empty() => r,
            _ => input_roles,
        }
    } else {
        input_roles
    };

    let cmd = RegisterAgentCommand {
        org_id: org_id.clone(),
        project: params.project.clone(),
        namespace: namespace.clone(),
        alias: params.alias.clone(),
        roles,
        description: params.description.clone(),
        agent_type: params.agent_type.clone(),
        metadata: params.metadata.unwrap_or_default(),
    };

    match h.container.app.register_agent.execute(cmd).await {
        Ok(response) => {
            let agent = &response.agent;
            let org = orchy_core::organization::OrganizationId::new(&org_id)
                .map_err(|e| e.to_string())?;
            let agent_id =
                orchy_core::agent::AgentId::from_str(&agent.id).map_err(|e| e.to_string())?;
            let ns = orchy_core::namespace::Namespace::try_from(agent.namespace.clone())
                .map_err(|e| e.to_string())?;
            h.set_session(agent_id, org, project, ns).await;
            Ok(to_json(&response))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn session_status(h: &OrchyHandler) -> Result<String, String> {
    let agent_id = h.get_session_agent().await;
    let agent_id_str = agent_id.as_ref().map(|id| id.to_string());
    let payload = serde_json::json!({
        "mcp_session_registered_with_orchy": agent_id.is_some(),
        "id": agent_id_str,
        "project": h.get_session_project().await.map(|p| p.to_string()),
        "namespace": h.get_session_namespace().await.map(|n| n.to_string()),
        "after_orchy_or_mcp_restart": concat!(
            "MCP Streamable HTTP session state is ephemeral. After orchy or the MCP client ",
            "restarts, you get a new MCP session. Persist your agent id from the last ",
            "register_agent response (or handoff knowledge), then call register_agent again ",
            "with the same project, description, namespace, and id. That re-binds this ",
            "MCP session to the existing agent; tasks, mailbox, and knowledge stay tied to that id."
        ),
    });
    Ok(to_json(&payload))
}

pub(super) async fn list_agents(
    h: &OrchyHandler,
    params: ListAgentsParams,
) -> Result<String, String> {
    let (org, project) = match h.require_session().await {
        Ok((_, org, proj, _)) => {
            let project = match params.project.as_deref() {
                Some(p) => parse_project(p)?,
                None => proj,
            };
            (org.to_string(), project.to_string())
        }
        Err(_) => {
            let p = params
                .project
                .as_deref()
                .ok_or("pass project or register first")?;
            let project = parse_project(p)?;
            (default_org().to_string(), project.to_string())
        }
    };

    let cmd = ListAgentsCommand {
        org_id: org,
        project: Some(project),
        after: None,
        limit: None,
    };
    match h.container.app.list_agents.execute(cmd).await {
        Ok(page) => Ok(to_json(&page)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn change_roles(
    h: &OrchyHandler,
    params: ChangeRolesParams,
) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;

    let cmd = ChangeRolesCommand {
        agent_id: agent_id.to_string(),
        roles: params.roles,
    };
    match h.container.app.change_roles.execute(cmd).await {
        Ok(agent) => Ok(to_json(&agent)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn heartbeat(h: &OrchyHandler) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;

    let cmd = HeartbeatCommand {
        agent_id: agent_id.to_string(),
    };
    match h.container.app.heartbeat.execute(cmd).await {
        Ok(()) => Ok(r#"{"ok":true}"#.to_string()),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn rename_alias(h: &OrchyHandler, new_alias: String) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;
    let cmd = RenameAliasCommand {
        agent_id: agent_id.to_string(),
        new_alias,
    };
    match h.container.app.rename_alias.execute(cmd).await {
        Ok(response) => Ok(to_json(&response)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn switch_context(
    h: &OrchyHandler,
    params: SwitchContextParams,
) -> Result<String, String> {
    let (agent_id, org, current_project, _) = h.require_session().await?;

    if params.project.is_none() && params.namespace.is_none() {
        return Err("at least one of project or namespace is required".to_string());
    }

    if let Some(ref ns) = params.namespace {
        let target_project = params
            .project
            .as_deref()
            .map(parse_project)
            .transpose()?
            .unwrap_or(current_project.clone());
        let _ = h
            .resolve_namespace_for(
                Some(ns),
                NamespacePolicy::RegisterIfNew,
                Some(&org),
                Some(&target_project),
            )
            .await;
    }

    let cmd = SwitchContextCommand {
        org_id: org.to_string(),
        agent_id: agent_id.to_string(),
        project: params.project.clone(),
        namespace: params.namespace.map(|ns| {
            if ns.starts_with('/') {
                ns
            } else {
                format!("/{ns}")
            }
        }),
    };

    match h.container.app.switch_context.execute(cmd).await {
        Ok(response) => {
            let project = parse_project(&response.project)?;
            let ns = orchy_core::namespace::Namespace::try_from(response.namespace.clone())
                .map_err(|e| e.to_string())?;
            h.set_session_project_and_namespace(project, ns).await;
            Ok(to_json(&response))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn get_agent_context(
    h: &OrchyHandler,
    params: GetAgentContextParams,
) -> Result<String, String> {
    let (agent_id, org, _, _) = h.require_session().await?;

    let relations_opts = parse_relation_options(params.relations);

    if let Some(opts) = relations_opts {
        let cmd = GetAgentCommand {
            agent_id: agent_id.to_string(),
            org_id: Some(org.to_string()),
            relations: Some(opts),
        };
        return match h.container.app.get_agent.execute(cmd).await {
            Ok(resp) => Ok(to_json(&resp)),
            Err(e) => Err(mcp_error(e)),
        };
    }

    let cmd = GetAgentSummaryCommand {
        org_id: org.to_string(),
        agent_id: agent_id.to_string(),
    };

    match h.container.app.get_agent_summary.execute(cmd).await {
        Ok(summary) => Ok(to_json(&summary)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn poll_updates(
    h: &OrchyHandler,
    params: PollUpdatesParams,
) -> Result<String, String> {
    let (_, _, session_project, _) = h.require_session().await?;
    let project = params
        .project
        .unwrap_or_else(|| session_project.to_string());

    let since = match params.since.as_deref() {
        Some(s) => s.to_string(),
        None => (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339(),
    };

    let cmd = PollUpdatesCommand {
        org_id: project.clone(),
        since: since.clone(),
        limit: params.limit,
    };

    match h.container.app.poll_updates.execute(cmd).await {
        Ok(events) => {
            let updates: Vec<_> = events
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "topic": e.topic,
                        "namespace": e.namespace,
                        "payload": e.payload,
                        "timestamp": e.timestamp.to_rfc3339(),
                    })
                })
                .collect();

            let result = serde_json::json!({
                "since": since,
                "count": updates.len(),
                "events": updates,
            });

            Ok(to_json(&result))
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn check_mailbox(
    h: &OrchyHandler,
    params: CheckMailboxParams,
) -> Result<String, String> {
    let (agent_id, org, session_project, _) = h.require_session().await?;

    let project = if let Some(p) = params.project {
        p
    } else {
        session_project.to_string()
    };

    let _namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let cmd = CheckMailboxCommand {
        agent_id: agent_id.to_string(),
        org_id: org.to_string(),
        project,
        after: params.after,
        limit: params.limit,
    };

    match h.container.app.check_mailbox.execute(cmd).await {
        Ok(page) => Ok(to_json(&page)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn check_sent_messages(
    h: &OrchyHandler,
    params: CheckSentMessagesParams,
) -> Result<String, String> {
    let (agent_id, org, session_project, _) = h.require_session().await?;

    let project = if let Some(p) = params.project {
        p
    } else {
        session_project.to_string()
    };

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let cmd = CheckSentMessagesCommand {
        agent_id: agent_id.to_string(),
        org_id: org.to_string(),
        project,
        namespace: Some(namespace.to_string()),
        after: params.after,
        limit: params.limit,
    };

    match h.container.app.check_sent_messages.execute(cmd).await {
        Ok(page) => Ok(to_json(&page)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn mark_read(h: &OrchyHandler, params: MarkReadParams) -> Result<String, String> {
    let (agent_id, _, _, _) = h.require_session().await?;

    let cmd = MarkReadCommand {
        agent_id: agent_id.to_string(),
        message_ids: params.message_ids,
    };

    match h.container.app.mark_read.execute(cmd).await {
        Ok(()) => Ok(r#"{"ok":true}"#.to_string()),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn list_conversation(
    h: &OrchyHandler,
    params: ListConversationParams,
) -> Result<String, String> {
    let (_, org, project, _) = h.require_session().await?;

    let cmd = ListConversationCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        message_id: params.message_id,
        limit: params.limit,
    };

    match h.container.app.list_conversation.execute(cmd).await {
        Ok(messages) => Ok(to_json(&messages)),
        Err(e) => Err(mcp_error(e)),
    }
}
