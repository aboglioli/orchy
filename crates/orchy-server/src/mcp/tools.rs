use std::collections::HashMap;
use std::str::FromStr;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};

use orchy_application::{
    AddDependencyCommand, AddTaskNoteCommand, AppendKnowledgeCommand, AssignTaskCommand,
    CancelTaskCommand, ChangeKnowledgeKindCommand, ChangeRolesCommand, CheckLockCommand,
    CheckMailboxCommand, CheckSentMessagesCommand, ClaimTaskCommand, CompleteTaskCommand,
    DelegateTaskCommand, DeleteKnowledgeCommand, DisconnectAgentCommand, FailTaskCommand,
    GetNextTaskCommand, GetProjectCommand, GetTaskWithContextCommand, HeartbeatCommand,
    ImportKnowledgeCommand, ListAgentsCommand, ListConversationCommand, ListKnowledgeCommand,
    ListNamespacesCommand, ListTagsCommand, ListTasksCommand, LockResourceCommand, MarkReadCommand,
    MergeTasksCommand, MoveKnowledgeCommand, MoveTaskCommand, PatchKnowledgeMetadataCommand,
    PollUpdatesCommand, PostTaskCommand, ReadKnowledgeCommand, RegisterAgentCommand,
    ReleaseTaskCommand, RemoveDependencyCommand, RenameKnowledgeCommand, ReplaceTaskCommand,
    ResourceRefInput, SearchKnowledgeCommand, SendMessageCommand, SetProjectMetadataCommand,
    SplitTaskCommand, StartTaskCommand, SubtaskInput, SwitchContextCommand, TagKnowledgeCommand,
    TagTaskCommand, UnblockTaskCommand, UnlockResourceCommand, UntagKnowledgeCommand,
    UntagTaskCommand, UpdateProjectCommand, UpdateTaskCommand, WriteKnowledgeCommand,
};
use orchy_core::knowledge::KnowledgeKind;

use super::handler::{
    NamespacePolicy, OrchyHandler, default_org, mcp_error, parse_project, to_json,
};
use super::params::*;

fn knowledge_metadata_from_json_str(
    raw: Option<&str>,
    label: &'static str,
) -> Result<HashMap<String, String>, String> {
    match raw {
        None | Some("") => Ok(HashMap::new()),
        Some(s) => serde_json::from_str(s).map_err(|e| format!("invalid {label} JSON: {e}")),
    }
}

fn optional_knowledge_metadata(
    raw: Option<String>,
    label: &'static str,
) -> Result<Option<HashMap<String, String>>, String> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => serde_json::from_str(s)
            .map(Some)
            .map_err(|e| format!("invalid {label} JSON: {e}")),
    }
}

#[tool_router]
impl OrchyHandler {
    #[tool(
        description = "Register as an agent. Required before almost every other tool. \
        Roles are optional — orchy assigns them from pending task demand if omitted. \
        Pass id to resume the same agent after a new MCP session (orchy or client restarted). \
        Use parent_id for agent lineage."
    )]
    async fn register_agent(
        &self,
        Parameters(params): Parameters<RegisterAgentParams>,
    ) -> Result<String, String> {
        if params.project.is_empty() {
            return Err(
                "project is required: call register_agent with project=\"<name>\" and description=\"<what this agent does>\""
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

        let _ = self
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
            match self.container.app.suggest_roles.execute(cmd).await {
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
            roles,
            description: params.description.unwrap_or_default(),
            id: params.id.clone(),
            parent_id: params.parent_id.clone(),
            metadata: params.metadata.unwrap_or_default(),
        };

        match self.container.app.register_agent.execute(cmd).await {
            Ok(response) => {
                let org = orchy_core::organization::OrganizationId::new(&org_id)
                    .map_err(|e| e.to_string())?;
                let agent_id = orchy_core::agent::AgentId::from_str(&response.id)
                    .map_err(|e| e.to_string())?;
                let ns = orchy_core::namespace::Namespace::try_from(response.namespace.clone())
                    .map_err(|e| e.to_string())?;
                self.set_session(agent_id, org, project, ns).await;
                Ok(to_json(&response))
            }
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Whether this MCP session is bound to an orchy agent, and how to resume \
        after an orchy or MCP transport restart. Does not require registration. Call after the \
        client has reconnected (new MCP initialize) if tools failed with session errors or you \
        are unsure whether you still need register_agent."
    )]
    async fn session_status(&self) -> Result<String, String> {
        let agent_id = self.get_session_agent();
        let agent_id_str = agent_id.as_ref().map(|id| id.to_string());
        let payload = serde_json::json!({
            "mcp_session_registered_with_orchy": agent_id.is_some(),
            "id": agent_id_str,
            "project": self.get_session_project().map(|p| p.to_string()),
            "namespace": self.get_session_namespace().map(|n| n.to_string()),
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

    #[tool(
        description = "List agents in a project. Works before registration if project is passed."
    )]
    async fn list_agents(
        &self,
        Parameters(params): Parameters<ListAgentsParams>,
    ) -> Result<String, String> {
        let (org, project) = match self.require_session() {
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
        match self.container.app.list_agents.execute(cmd).await {
            Ok(page) => Ok(to_json(&page)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Change the roles of the session agent. Affects which tasks \
        get_next_task returns."
    )]
    async fn change_roles(
        &self,
        Parameters(params): Parameters<ChangeRolesParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = ChangeRolesCommand {
            agent_id: agent_id.to_string(),
            roles: params.roles,
        };
        match self.container.app.change_roles.execute(cmd).await {
            Ok(agent) => Ok(to_json(&agent)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Send a heartbeat for the session agent to signal liveness.")]
    async fn heartbeat(&self) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = HeartbeatCommand {
            agent_id: agent_id.to_string(),
        };
        match self.container.app.heartbeat.execute(cmd).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Disconnect and release all claimed tasks back to pending. \
        Call this when your session is ending."
    )]
    async fn disconnect(&self) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = DisconnectAgentCommand {
            agent_id: agent_id.to_string(),
        };
        match self.container.app.disconnect_agent.execute(cmd).await {
            Ok(()) => Ok("disconnected".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Switch the session agent to a different project, namespace, or both \
        within the same organization. \
        If only project is given, namespace resets to root. \
        If only namespace is given, stays in current project. \
        Switching projects releases claimed tasks, locks, and watchers in the old project."
    )]
    async fn switch_context(
        &self,
        Parameters(params): Parameters<SwitchContextParams>,
    ) -> Result<String, String> {
        let (agent_id, org, current_project, _) = self.require_session()?;

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
            let _ = self
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

        match self.container.app.switch_context.execute(cmd).await {
            Ok(response) => {
                let project = parse_project(&response.project)?;
                let ns = orchy_core::namespace::Namespace::try_from(response.namespace.clone())
                    .map_err(|e| e.to_string())?;
                self.set_session_project_and_namespace(project, ns);
                Ok(to_json(&response))
            }
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Create a task. Use parent_id to create a subtask. \
        Tasks with depends_on are auto-blocked until dependencies complete. \
        Use refs to attach resource references (files, URLs, etc.).")]
    async fn post_task(
        &self,
        Parameters(params): Parameters<PostTaskParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
            .await?;

        let cmd = PostTaskCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            title: params.title,
            description: params.description,
            priority: params.priority,
            assigned_roles: params.assigned_roles,
            depends_on: params.depends_on,
            parent_id: params.parent_id,
            created_by: self.get_session_agent().map(|id| id.to_string()),
            refs: params.refs.map(|v| {
                v.into_iter()
                    .map(|r| ResourceRefInput {
                        kind: r.kind,
                        id: r.id,
                        display: r.display,
                    })
                    .collect()
            }),
        };

        match self.container.app.post_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Get the next available task matching your roles. \
        When claim is true (default), claims the task and returns full context (ancestors, children). \
        When claim is false, returns the top candidate without claiming (peek). \
        Skips tasks with incomplete dependencies.")]
    async fn get_next_task(
        &self,
        Parameters(params): Parameters<GetNextTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        let roles = match params.role {
            Some(r) => vec![r],
            None => match self
                .container
                .app
                .get_agent
                .execute(orchy_application::GetAgentCommand {
                    agent_id: agent_id.to_string(),
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

        match self.container.app.get_next_task.execute(cmd).await {
            Ok(Some(task)) => {
                let task_id = task
                    .id
                    .parse::<orchy_core::task::TaskId>()
                    .map_err(|e| e.to_string())?;
                let ctx = self
                    .container
                    .app
                    .get_task_with_context
                    .execute(GetTaskWithContextCommand {
                        task_id: task_id.to_string(),
                    })
                    .await
                    .map_err(mcp_error)?;
                Ok(to_json(&ctx))
            }
            Ok(None) => Ok("no tasks available".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "List tasks, optionally filtered by namespace, status, and parent_id. \
        Use parent_id to list subtasks of a specific task. \
        Defaults to session namespace; pass namespace=/ to see all namespaces."
    )]
    async fn list_tasks(
        &self,
        Parameters(params): Parameters<ListTasksParams>,
    ) -> Result<String, String> {
        let (_, org, session_project, _) = self.require_session()?;

        let project = params
            .project
            .unwrap_or_else(|| session_project.to_string());

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        let cmd = ListTasksCommand {
            org_id: org.to_string(),
            project: Some(project),
            namespace: Some(namespace.to_string()),
            status: params.status,
            parent_id: params.parent_id,
            assigned_to: None,
            tag: None,
            after: params.after,
            limit: params.limit,
        };

        match self.container.app.list_tasks.execute(cmd).await {
            Ok(page) => Ok(to_json(&page)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Claim a specific task for the session agent. \
        When start is true, moves claimed → in_progress in the same call.")]
    async fn claim_task(
        &self,
        Parameters(params): Parameters<ClaimTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = ClaimTaskCommand {
            task_id: params.task_id.clone(),
            agent_id: agent_id.to_string(),
            start: params.start,
        };

        match self.container.app.claim_task.execute(cmd).await {
            Ok(task) => {
                let task_id = task
                    .id
                    .parse::<orchy_core::task::TaskId>()
                    .map_err(|e| e.to_string())?;
                let ctx = self
                    .container
                    .app
                    .get_task_with_context
                    .execute(GetTaskWithContextCommand {
                        task_id: task_id.to_string(),
                    })
                    .await
                    .map_err(mcp_error)?;
                Ok(to_json(&ctx))
            }
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Start a claimed task (claimed → in_progress). \
        Must be claimed by you first. Returns task with full context.")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<StartTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = StartTaskCommand {
            task_id: params.task_id.clone(),
            agent_id: agent_id.to_string(),
        };

        match self.container.app.start_task.execute(cmd).await {
            Ok(task) => {
                let task_id = task
                    .id
                    .parse::<orchy_core::task::TaskId>()
                    .map_err(|e| e.to_string())?;
                let ctx = self
                    .container
                    .app
                    .get_task_with_context
                    .execute(GetTaskWithContextCommand {
                        task_id: task_id.to_string(),
                    })
                    .await
                    .map_err(mcp_error)?;
                Ok(to_json(&ctx))
            }
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Mark a task as completed. Always include a summary: what was done, \
        what was learned, key decisions. Write important findings to memory/documents too."
    )]
    async fn complete_task(
        &self,
        Parameters(params): Parameters<CompleteTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = CompleteTaskCommand {
            task_id: params.task_id,
            summary: params.summary,
        };

        match self.container.app.complete_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Mark a task as failed with an optional reason.")]
    async fn fail_task(
        &self,
        Parameters(params): Parameters<FailTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = FailTaskCommand {
            task_id: params.task_id,
            reason: params.reason,
        };

        match self.container.app.fail_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Cancel a task (pending, claimed, in_progress, or blocked). \
        Dependent tasks that were blocked on it are notified."
    )]
    async fn cancel_task(
        &self,
        Parameters(params): Parameters<CancelTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = CancelTaskCommand {
            task_id: params.task_id,
            reason: params.reason,
        };

        match self.container.app.cancel_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Update task title, description, and/or priority. \
        Must be pending, claimed, in_progress, or blocked.")]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = UpdateTaskCommand {
            task_id: params.task_id,
            title: params.title,
            description: params.description,
            priority: params.priority,
        };

        match self.container.app.update_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Manually unblock a blocked task (e.g. after resolving an external dependency)."
    )]
    async fn unblock_task(
        &self,
        Parameters(params): Parameters<UnblockTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = UnblockTaskCommand {
            task_id: params.task_id,
        };

        match self.container.app.unblock_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Assign a claimed or in-progress task to a different agent. \
        The task must already be claimed."
    )]
    async fn assign_task(
        &self,
        Parameters(params): Parameters<AssignTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let agent_id = self.resolve_agent_id(&params.agent).await?;

        let cmd = AssignTaskCommand {
            task_id: params.task_id,
            agent_id: agent_id.to_string(),
        };

        match self.container.app.assign_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Send a message. Target: agent UUID, 'role:name' (all agents \
        with that role), or 'broadcast' (all agents except you). \
        Use refs to attach resource references (files, URLs, etc.)."
    )]
    async fn send_message(
        &self,
        Parameters(params): Parameters<SendMessageParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
            .await?;

        let to = match orchy_core::message::MessageTarget::parse(&params.to) {
            Ok(_) => params.to.clone(),
            Err(_) => match self.resolve_agent_id(&params.to).await {
                Ok(id) => id.to_string(),
                Err(_) => {
                    return Err(format!(
                        "invalid target: '{}' (not a UUID, role:name, broadcast, or known alias)",
                        params.to
                    ));
                }
            },
        };

        let cmd = SendMessageCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            from_agent_id: agent_id.to_string(),
            to,
            body: params.body,
            reply_to: params.reply_to,
            refs: params.refs.map(|v| {
                v.into_iter()
                    .map(|r| ResourceRefInput {
                        kind: r.kind,
                        id: r.id,
                        display: r.display,
                    })
                    .collect()
            }),
        };

        match self.container.app.send_message.execute(cmd).await {
            Ok(message) => Ok(to_json(&message)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Check your mailbox for incoming messages. Returns unread and recent \
        messages addressed to you."
    )]
    async fn check_mailbox(
        &self,
        Parameters(params): Parameters<CheckMailboxParams>,
    ) -> Result<String, String> {
        let (agent_id, org, session_project, _) = self.require_session()?;

        let project = if let Some(p) = params.project {
            p
        } else {
            session_project.to_string()
        };

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        let cmd = CheckMailboxCommand {
            agent_id: agent_id.to_string(),
            org_id: org.to_string(),
            project,
            namespace: Some(namespace.to_string()),
            after: params.after,
            limit: params.limit,
        };

        match self.container.app.check_mailbox.execute(cmd).await {
            Ok(page) => Ok(to_json(&page)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "List messages you have sent, with delivery and read status.")]
    async fn check_sent_messages(
        &self,
        Parameters(params): Parameters<CheckSentMessagesParams>,
    ) -> Result<String, String> {
        let (agent_id, org, session_project, _) = self.require_session()?;

        let project = if let Some(p) = params.project {
            p
        } else {
            session_project.to_string()
        };

        let namespace = self
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

        match self.container.app.check_sent_messages.execute(cmd).await {
            Ok(page) => Ok(to_json(&page)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Mark messages as read by their IDs for the session agent.")]
    async fn mark_read(
        &self,
        Parameters(params): Parameters<MarkReadParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = MarkReadCommand {
            agent_id: agent_id.to_string(),
            message_ids: params.message_ids,
        };

        match self.container.app.mark_read.execute(cmd).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "List the full conversation thread for a given message ID. \
        Walks the reply_to chain to find the root, then returns all messages in \
        the thread in chronological order. Use limit to cap the number of messages \
        returned (most recent N)."
    )]
    async fn list_conversation(
        &self,
        Parameters(params): Parameters<ListConversationParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let cmd = ListConversationCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            message_id: params.message_id,
            limit: params.limit,
        };

        match self.container.app.list_conversation.execute(cmd).await {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Add a note to a task. Creates a knowledge entry linked to the task via ResourceRef."
    )]
    async fn add_task_note(
        &self,
        Parameters(params): Parameters<AddTaskNoteParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = AddTaskNoteCommand {
            task_id: params.task_id,
            body: params.body,
            author: Some(agent_id.to_string()),
        };

        match self.container.app.add_task_note.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Split a task into subtasks. The parent task is blocked and will \
        auto-complete when all subtasks finish. Agents should work on subtasks directly, \
        not the parent. Returns the parent (with updated status) and all created subtasks."
    )]
    async fn split_task(
        &self,
        Parameters(params): Parameters<SplitTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let subtasks = params
            .subtasks
            .into_iter()
            .map(|sp| SubtaskInput {
                title: sp.title,
                description: sp.description,
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

        match self.container.app.split_task.execute(cmd).await {
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

    #[tool(
        description = "Replace a task with new tasks. Cancels the original and creates \
        replacements that inherit the original's parent (if any)."
    )]
    async fn replace_task(
        &self,
        Parameters(params): Parameters<ReplaceTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let replacements = params
            .replacements
            .into_iter()
            .map(|sp| SubtaskInput {
                title: sp.title,
                description: sp.description,
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

        match self.container.app.replace_task.execute(cmd).await {
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

    #[tool(
        description = "Merge multiple tasks into one. Source tasks must be pending, \
        blocked, or claimed. They are cancelled and a new consolidated task is created \
        with the highest priority, combined roles, and combined dependencies. \
        Children of source tasks are re-parented. Tasks depending on sources are updated."
    )]
    async fn merge_tasks(
        &self,
        Parameters(params): Parameters<MergeTasksParams>,
    ) -> Result<String, String> {
        let (agent_id, org, _, _) = self.require_session()?;

        let cmd = MergeTasksCommand {
            org_id: org.to_string(),
            task_ids: params.task_ids,
            title: params.title,
            description: params.description,
            created_by: Some(agent_id.to_string()),
        };

        match self.container.app.merge_tasks.execute(cmd).await {
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

    #[tool(
        description = "Create a subtask under a claimed/in-progress task without blocking the parent. \
        Unlike split_task, the parent keeps its status."
    )]
    async fn delegate_task(
        &self,
        Parameters(params): Parameters<DelegateTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let cmd = DelegateTaskCommand {
            task_id: params.task_id,
            title: params.title,
            description: params.description,
            priority: params.priority,
            assigned_roles: params.assigned_roles,
            created_by: Some(agent_id.to_string()),
        };

        match self.container.app.delegate_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Add a dependency to a task. If the dependency is not yet completed, \
        the task will be blocked."
    )]
    async fn add_dependency(
        &self,
        Parameters(params): Parameters<AddDependencyParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = AddDependencyCommand {
            task_id: params.task_id,
            dependency_id: params.dependency_id,
        };

        match self.container.app.add_dependency.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Remove a dependency from a task. If all remaining dependencies are \
        completed, the task will be unblocked."
    )]
    async fn remove_dependency(
        &self,
        Parameters(params): Parameters<RemoveDependencyParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = RemoveDependencyCommand {
            task_id: params.task_id,
            dependency_id: params.dependency_id,
        };

        match self.container.app.remove_dependency.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Get the project metadata for the current session's project. \
        Set include_summary to add agent/task overview (same data as the former get_project_summary)."
    )]
    async fn get_project(
        &self,
        Parameters(params): Parameters<GetProjectParams>,
    ) -> Result<String, String> {
        let (_, org, project_id, _) = self.require_session()?;

        let cmd = GetProjectCommand {
            org_id: org.to_string(),
            project: project_id.to_string(),
        };

        let project = self
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
        let project_agents: Vec<_> = self
            .container
            .app
            .list_agents
            .execute(agents_cmd)
            .await
            .map_err(mcp_error)?
            .items
            .into_iter()
            .filter(|a| a.status != "disconnected")
            .collect();

        let tasks_cmd = ListTasksCommand {
            org_id: org.to_string(),
            project: Some(project_id.to_string()),
            namespace: None,
            status: None,
            parent_id: None,
            assigned_to: None,
            tag: None,
            after: None,
            limit: None,
        };
        let all_tasks = self
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

        let agent_id = self.get_session_agent();
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

    #[tool(
        description = "Update the project description and/or metadata for the current session's project."
    )]
    async fn update_project(
        &self,
        Parameters(params): Parameters<UpdateProjectParams>,
    ) -> Result<String, String> {
        let (_, org, project_id, _) = self.require_session()?;

        let project_cmd = GetProjectCommand {
            org_id: org.to_string(),
            project: project_id.to_string(),
        };
        let project = self
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

        match self.container.app.update_project.execute(cmd).await {
            Ok(project) => Ok(to_json(&project)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Set a metadata key-value pair on the current session's project.")]
    async fn set_project_metadata(
        &self,
        Parameters(params): Parameters<SetProjectMetadataParams>,
    ) -> Result<String, String> {
        let (_, org, project_id, _) = self.require_session()?;

        let cmd = SetProjectMetadataCommand {
            org_id: org.to_string(),
            project: project_id.to_string(),
            key: params.key,
            value: params.value,
        };

        match self.container.app.set_project_metadata.execute(cmd).await {
            Ok(project) => Ok(to_json(&project)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "List all registered namespaces for the current session's project. \
        Namespaces are auto-registered when agents connect or tasks are created."
    )]
    async fn list_namespaces(
        &self,
        Parameters(params): Parameters<ListNamespacesParams>,
    ) -> Result<String, String> {
        let (_, org, session_project, _) = self.require_session()?;
        let project = if let Some(p) = params.project {
            p
        } else {
            session_project.to_string()
        };

        let cmd = ListNamespacesCommand {
            org_id: org.to_string(),
            project,
        };

        match self.container.app.list_namespaces.execute(cmd).await {
            Ok(namespaces) => Ok(to_json(&namespaces)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Move a task to a different namespace within the same project.")]
    async fn move_task(
        &self,
        Parameters(params): Parameters<MoveTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let namespace = self
            .resolve_namespace(Some(&params.new_namespace), NamespacePolicy::RegisterIfNew)
            .await?;

        let cmd = MoveTaskCommand {
            task_id: params.task_id,
            new_namespace: namespace.to_string(),
        };

        match self.container.app.move_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Get everything you need in one call: your agent info, project metadata, \
        inbox messages, pending tasks matching your roles, skills, and handoff context from \
        previous sessions. Call this after register_agent to bootstrap quickly."
    )]
    async fn get_agent_context(
        &self,
        Parameters(_params): Parameters<GetAgentContextParams>,
    ) -> Result<String, String> {
        let (agent_id, org, _, _) = self.require_session()?;

        let cmd = orchy_application::GetAgentSummaryCommand {
            org_id: org.to_string(),
            agent_id: agent_id.to_string(),
        };

        match self.container.app.get_agent_summary.execute(cmd).await {
            Ok(summary) => Ok(to_json(&summary)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Add a tag to a task.")]
    async fn tag_task(
        &self,
        Parameters(params): Parameters<TagTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = TagTaskCommand {
            task_id: params.task_id,
            tag: params.tag,
        };

        match self.container.app.tag_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Remove a tag from a task.")]
    async fn untag_task(
        &self,
        Parameters(params): Parameters<UntagTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = UntagTaskCommand {
            task_id: params.task_id,
            tag: params.tag,
        };

        match self.container.app.untag_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Acquire a named distributed lock. Fails if held by another agent. \
        Locks auto-expire after ttl_secs (default 300)."
    )]
    async fn lock_resource(
        &self,
        Parameters(params): Parameters<LockResourceParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, _) = self.require_session()?;

        let namespace = self
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

        match self.container.app.lock_resource.execute(cmd).await {
            Ok(lock) => Ok(to_json(&lock)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Release a named distributed lock.")]
    async fn unlock_resource(
        &self,
        Parameters(params): Parameters<UnlockResourceParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = UnlockResourceCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            name: params.name,
            holder_agent_id: agent_id.to_string(),
        };

        match self.container.app.unlock_resource.execute(cmd).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Check if a resource lock exists without acquiring it.")]
    async fn check_lock(
        &self,
        Parameters(params): Parameters<CheckLockParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = CheckLockCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            name: params.name,
        };

        match self.container.app.check_lock.execute(cmd).await {
            Ok(Some(lock)) => Ok(to_json(&lock)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Release a claimed or in-progress task back to pending.")]
    async fn release_task(
        &self,
        Parameters(params): Parameters<ReleaseTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let cmd = ReleaseTaskCommand {
            task_id: params.task_id,
        };

        match self.container.app.release_task.execute(cmd).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "List all unique tags used across tasks. \
        Defaults to session namespace; pass namespace=/ to see all namespaces.")]
    async fn list_tags(
        &self,
        Parameters(params): Parameters<ListTagsParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        let cmd = ListTagsCommand {
            org_id: Some(org.to_string()),
            project: Some(project.to_string()),
            namespace: Some(namespace.to_string()),
        };

        match self.container.app.list_tags.execute(cmd).await {
            Ok(tags) => Ok(to_json(&tags)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Get a task by its ID with full context (ancestors and children).")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<GetTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        match self
            .container
            .app
            .get_task_with_context
            .execute(GetTaskWithContextCommand {
                task_id: params.task_id.clone(),
            })
            .await
        {
            Ok(ctx) => Ok(to_json(&ctx)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Poll for recent events in the project since a timestamp. \
        Returns domain events (task changes, messages, document updates, etc). \
        Use alongside check_mailbox for full reactivity."
    )]
    async fn poll_updates(
        &self,
        Parameters(params): Parameters<PollUpdatesParams>,
    ) -> Result<String, String> {
        let (_, _, session_project, _) = self.require_session()?;
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

        match self.container.app.poll_updates.execute(cmd).await {
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

    #[tool(description = "List available knowledge entry types with descriptions.")]
    async fn list_knowledge_types(
        &self,
        Parameters(_params): Parameters<ListKnowledgeTypesParams>,
    ) -> Result<String, String> {
        let types: Vec<serde_json::Value> = KnowledgeKind::all()
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": t.to_string(),
                    "description": t.description(),
                })
            })
            .collect();
        Ok(to_json(&types))
    }

    #[tool(description = "Write a knowledge entry. Creates or updates by path. \
        kind is required — use list_knowledge_types for valid values (includes skill). \
        Optional `metadata` is a JSON object merged on update; `metadata_remove` drops keys first.")]
    async fn write_knowledge(
        &self,
        Parameters(params): Parameters<WriteKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
            .await?;

        let metadata = knowledge_metadata_from_json_str(params.metadata.as_deref(), "metadata")?;

        let cmd = WriteKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            kind: params.kind,
            title: params.title,
            content: params.content,
            tags: params.tags,
            version: params.version,
            agent_id: self.get_session_agent().map(|id| id.to_string()),
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(metadata)
            },
            metadata_remove: params.metadata_remove,
            refs: params.refs.map(|v| {
                v.into_iter()
                    .map(|r| ResourceRefInput {
                        kind: r.kind,
                        id: r.id,
                        display: r.display,
                    })
                    .collect()
            }),
        };

        match self.container.app.write_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "Merge or remove knowledge entry metadata without changing title, content, or kind. \
        `metadata` is a JSON object of string values; `metadata_remove` lists keys to delete first."
    )]
    async fn patch_knowledge_metadata(
        &self,
        Parameters(params): Parameters<PatchKnowledgeMetadataParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let set = optional_knowledge_metadata(params.metadata, "metadata")?.unwrap_or_default();
        let remove = params.metadata_remove.unwrap_or_default();

        let cmd = PatchKnowledgeMetadataCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            set,
            remove,
            version: params.version,
        };

        match self
            .container
            .app
            .patch_knowledge_metadata
            .execute(cmd)
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Read a knowledge entry by path. \
        Defaults to session namespace; pass namespace=/ for root.")]
    async fn read_knowledge(
        &self,
        Parameters(params): Parameters<ReadKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, session_project, _) = self.require_session()?;
        let project = params
            .project
            .unwrap_or_else(|| session_project.to_string());

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        let cmd = ReadKnowledgeCommand {
            org_id: org.to_string(),
            project,
            namespace: Some(namespace.to_string()),
            path: params.path,
        };

        match self.container.app.read_knowledge.execute(cmd).await {
            Ok(Some(entry)) => Ok(to_json(&entry)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(
        description = "List knowledge entries with optional filters: type, tag, \
        path_prefix, agent_id, namespace."
    )]
    async fn list_knowledge(
        &self,
        Parameters(params): Parameters<ListKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, _, _) = self.require_session()?;

        let namespace = match params.namespace.as_deref() {
            Some(_) => Some(
                self.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
                    .await?
                    .to_string(),
            ),
            None => None,
        };

        let agent_id = match params.agent.as_deref() {
            Some(s) => Some(self.resolve_agent_id(s).await?.to_string()),
            None => None,
        };

        let project = if params.project.is_some() {
            params.project
        } else if namespace.is_none() {
            self.get_session_project().map(|p| p.to_string())
        } else {
            None
        };

        let cmd = ListKnowledgeCommand {
            org_id: org.to_string(),
            project,
            include_org_level: false,
            namespace,
            kind: params.kind,
            tag: params.tag,
            path_prefix: params.path_prefix,
            agent_id,
            after: params.after,
            limit: params.limit,
        };

        match self.container.app.list_knowledge.execute(cmd).await {
            Ok(page) => Ok(to_json(&page)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Search knowledge entries by semantic similarity. \
        Defaults to session namespace; pass namespace=/ to search all namespaces.")]
    async fn search_knowledge(
        &self,
        Parameters(params): Parameters<SearchKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, _, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        let cmd = SearchKnowledgeCommand {
            org_id: org.to_string(),
            query: params.query,
            namespace: Some(namespace.to_string()),
            kind: params.kind,
            limit: params.limit,
            project: params.project,
        };

        match self.container.app.search_knowledge.execute(cmd).await {
            Ok(entries) => Ok(to_json(&entries)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Delete a knowledge entry by path.")]
    async fn delete_knowledge(
        &self,
        Parameters(params): Parameters<DeleteKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = DeleteKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
        };

        match self.container.app.delete_knowledge.execute(cmd).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Append text to a knowledge entry. Creates if it doesn't exist.")]
    async fn append_knowledge(
        &self,
        Parameters(params): Parameters<AppendKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
            .await?;

        let metadata = optional_knowledge_metadata(params.metadata, "metadata")?;

        let cmd = AppendKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            kind: params.kind,
            value: params.value,
            separator: params.separator,
            agent_id: self.get_session_agent().map(|id| id.to_string()),
            metadata,
            metadata_remove: params.metadata_remove,
        };

        match self.container.app.append_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Move a knowledge entry to a different namespace.")]
    async fn move_knowledge(
        &self,
        Parameters(params): Parameters<MoveKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let new_namespace = self
            .resolve_namespace(Some(&params.new_namespace), NamespacePolicy::RegisterIfNew)
            .await?;

        let cmd = MoveKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            new_namespace: new_namespace.to_string(),
        };

        match self.container.app.move_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Rename a knowledge entry's path.")]
    async fn rename_knowledge(
        &self,
        Parameters(params): Parameters<RenameKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = RenameKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            new_path: params.new_path,
        };

        match self.container.app.rename_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Change the kind of an existing knowledge entry. \
        Does not run on `write_knowledge` updates — use this tool explicitly. \
        Bumps version when the kind actually changes.")]
    async fn change_knowledge_kind(
        &self,
        Parameters(params): Parameters<ChangeKnowledgeKindParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = ChangeKnowledgeKindCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            new_kind: params.kind,
            version: params.version,
        };

        match self.container.app.change_knowledge_kind.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Add a tag to a knowledge entry.")]
    async fn tag_knowledge(
        &self,
        Parameters(params): Parameters<TagKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = TagKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            tag: params.tag,
        };

        match self.container.app.tag_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Remove a tag from a knowledge entry.")]
    async fn untag_knowledge(
        &self,
        Parameters(params): Parameters<UntagKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let cmd = UntagKnowledgeCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
            path: params.path,
            tag: params.tag,
        };

        match self.container.app.untag_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }

    #[tool(description = "Import a knowledge entry from a linked project.")]
    async fn import_knowledge(
        &self,
        Parameters(params): Parameters<ImportKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(None, NamespacePolicy::RegisterIfNew)
            .await?;

        let cmd = ImportKnowledgeCommand {
            source_org_id: org.to_string(),
            source_project: params.source_project,
            source_namespace: params.source_namespace,
            source_path: params.path,
            target_org_id: org.to_string(),
            target_project: project.to_string(),
            target_namespace: Some(namespace.to_string()),
            target_path: None,
            agent_id: self.get_session_agent().map(|id| id.to_string()),
        };

        match self.container.app.import_knowledge.execute(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(mcp_error(e)),
        }
    }
}

use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, PaginatedRequestParams, Prompt, PromptMessage,
    PromptMessageRole, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
type ListPromptsResult = rmcp::model::ListPromptsResult;

impl ServerHandler for OrchyHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_instructions(super::handler::INSTRUCTIONS.to_string())
    }

    async fn initialize(
        &self,
        _request: rmcp::model::InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, ErrorData> {
        if let Some(session_id) = extract_session_id(&context) {
            self.set_mcp_session_id(session_id);
        }
        Ok(rmcp::model::InitializeResult::new(
            self.get_info().capabilities,
        ))
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        if let Some(session_id) = extract_session_id(&context) {
            self.set_mcp_session_id(session_id);
        }
        self.touch_heartbeat();
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        if let Some(session_id) = extract_session_id(&context) {
            self.set_mcp_session_id(session_id);
        }
        self.touch_heartbeat();
        let tools = Self::tool_router()
            .list_all()
            .into_iter()
            .map(|mut t| {
                t.input_schema = super::schema_compat::compat_tool_input_schema(t.input_schema);
                t
            })
            .collect();
        Ok(rmcp::model::ListToolsResult {
            tools,
            meta: None,
            next_cursor: None,
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        if let Some(session_id) = extract_session_id(&context) {
            self.set_mcp_session_id(session_id);
        }
        self.touch_heartbeat();
        let (_, org, project, namespace) = match self.require_session() {
            Ok(s) => s,
            Err(_) => {
                return Ok(ListPromptsResult {
                    prompts: vec![],
                    meta: None,
                    next_cursor: None,
                });
            }
        };

        let cmd = orchy_application::ListSkillsCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
        };
        let skills = self
            .container
            .app
            .list_skills
            .execute(cmd)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let prompts = skills
            .into_iter()
            .map(|s| Prompt::new(s.title.clone(), Some(s.title.clone()), None))
            .collect();

        Ok(ListPromptsResult {
            prompts,
            meta: None,
            next_cursor: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        if let Some(session_id) = extract_session_id(&context) {
            self.set_mcp_session_id(session_id);
        }
        self.touch_heartbeat();
        let (_, org, project, namespace) = self
            .require_session()
            .map_err(|e| ErrorData::internal_error(e, None))?;

        let cmd = orchy_application::ListSkillsCommand {
            org_id: org.to_string(),
            project: project.to_string(),
            namespace: Some(namespace.to_string()),
        };
        let skills = self
            .container
            .app
            .list_skills
            .execute(cmd)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let entry = skills
            .into_iter()
            .find(|s| s.title == request.name)
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("skill '{}' not found", request.name), None)
            })?;

        let mut result = GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            entry.content.clone(),
        )]);
        result.description = Some(entry.title.clone());
        Ok(result)
    }
}

fn extract_session_id(context: &RequestContext<RoleServer>) -> Option<String> {
    context
        .extensions
        .get::<http::request::Parts>()
        .and_then(|parts: &http::request::Parts| {
            parts.uri.query().and_then(|query: &str| {
                query
                    .split('&')
                    .find(|s: &&str| s.starts_with("sessionId="))
                    .map(|s: &str| s["sessionId=".len()..].to_string())
            })
        })
}
