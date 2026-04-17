use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};

use orchy_core::agent::RegisterAgent;
use orchy_core::knowledge::service::PatchKnowledgeMetadata;
use orchy_core::knowledge::{
    KnowledgeFilter, KnowledgeKind, Version as KnowledgeVersion, WriteKnowledge,
};
use orchy_core::message::service::SendMessage;
use orchy_core::message::{MessageId, MessageTarget};
use orchy_core::namespace::{Namespace, NamespaceStore};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::service::RequestReviewCommand;
use orchy_core::task::{Priority, ReviewStore, Task, TaskFilter, TaskId, WatcherStore};

use super::handler::{
    NamespacePolicy, OrchyHandler, default_org, parse_agent_id, parse_message_id, parse_namespace,
    parse_project, parse_review_id, parse_task_id, to_json,
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
            Some(s) if !s.is_empty() => parse_namespace(&format!("/{s}"))?,
            _ => Namespace::root(),
        };

        let org_id = match params.organization.as_deref() {
            Some(s) if !s.is_empty() => OrganizationId::new(s).map_err(|e| e.to_string())?,
            _ => default_org(),
        };

        let _ =
            NamespaceStore::register(&*self.container.store, &org_id, &project, &namespace).await;

        let parent_id = params.parent_id.map(|s| parse_agent_id(&s)).transpose()?;

        let id = match params.id {
            Some(s) if !s.is_empty() => Some(parse_agent_id(&s)?),
            _ => None,
        };

        let input_roles = params.roles.unwrap_or_default();
        let roles = if input_roles.is_empty() {
            match self
                .container
                .task_service
                .suggest_roles(&project, Some(namespace.clone()))
                .await
            {
                Ok(r) if !r.is_empty() => r,
                _ => input_roles,
            }
        } else {
            input_roles
        };

        let cmd = RegisterAgent {
            org_id: org_id.clone(),
            project: project.clone(),
            namespace: namespace.clone(),
            roles,
            description: params.description.unwrap_or_default(),
            id,
            parent_id,
            metadata: params.metadata.unwrap_or_default(),
        };

        match self.container.agent_service.register(cmd).await {
            Ok(agent) => {
                self.set_session(agent.id().clone(), org_id, project, namespace)
                    .await;
                Ok(to_json(&agent))
            }
            Err(e) => Err(e.to_string()),
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
        let project = match params.project.as_deref() {
            Some(p) => parse_project(p)?,
            None => self
                .get_session_project()
                .ok_or("pass project or register first")?,
        };

        let (_, org, _, _) = self.require_session()?;
        match self.container.agent_service.list(&org).await {
            Ok(agents) => {
                let filtered: Vec<_> = agents
                    .into_iter()
                    .filter(|a| *a.project() == project)
                    .collect();
                Ok(to_json(&filtered))
            }
            Err(e) => Err(e.to_string()),
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

        match self
            .container
            .agent_service
            .change_roles(&agent_id, params.roles)
            .await
        {
            Ok(agent) => Ok(to_json(&agent)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Send a heartbeat for the session agent to signal liveness.")]
    async fn heartbeat(&self) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        match self.container.agent_service.heartbeat(&agent_id).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Disconnect and release all claimed tasks back to pending. \
        Call this when your session is ending."
    )]
    async fn disconnect(&self) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        if let Err(e) = self
            .container
            .task_service
            .release_agent_tasks(&agent_id)
            .await
        {
            return Err(e.to_string());
        }

        let _ = self
            .container
            .lock_service
            .release_agent_locks(&agent_id)
            .await;

        let watchers = WatcherStore::find_by_agent(&*self.container.store, &agent_id)
            .await
            .unwrap_or_default();
        for w in &watchers {
            let _ = WatcherStore::delete(&*self.container.store, &w.task_id(), &agent_id).await;
        }

        let reviews = ReviewStore::find_pending_for_agent(&*self.container.store, &agent_id)
            .await
            .unwrap_or_default();
        for mut r in reviews {
            r.unassign_reviewer();
            let _ = ReviewStore::save(&*self.container.store, &mut r).await;
        }

        match self.container.agent_service.disconnect(&agent_id).await {
            Ok(()) => Ok("disconnected".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Switch the session agent to a different project, namespace, or both \
        within the same organization. \
        If only project is given, namespace resets to root. \
        If only namespace is given, stays in current project. \
        Switching projects releases claimed tasks, locks, watchers, and reviews in the old project."
    )]
    async fn switch_context(
        &self,
        Parameters(params): Parameters<SwitchContextParams>,
    ) -> Result<String, String> {
        let (agent_id, org, current_project, current_namespace) = self.require_session()?;

        if params.project.is_none() && params.namespace.is_none() {
            return Err("at least one of project or namespace is required".to_string());
        }

        let target_project = match &params.project {
            Some(p) => Some(parse_project(p)?),
            None => None,
        };

        let project_changed = target_project
            .as_ref()
            .is_some_and(|p| *p != current_project);

        let register_project = target_project.as_ref().unwrap_or(&current_project);
        let target_namespace = match &params.namespace {
            Some(ns) => {
                self.resolve_namespace_for(
                    Some(ns),
                    NamespacePolicy::RegisterIfNew,
                    Some(&org),
                    Some(register_project),
                )
                .await?
            }
            None if project_changed => Namespace::root(),
            None => current_namespace,
        };

        if project_changed {
            let tasks = self
                .container
                .task_service
                .list(TaskFilter {
                    assigned_to: Some(agent_id.clone()),
                    project: Some(current_project.clone()),
                    ..Default::default()
                })
                .await
                .unwrap_or_default();
            for task in &tasks {
                let _ = self.container.task_service.release(&task.id()).await;
            }

            let locks = LockStore::find_by_holder(&*self.container.store, &agent_id)
                .await
                .unwrap_or_default();
            for lock in locks {
                if *lock.project() == current_project {
                    let _ = self
                        .container
                        .lock_service
                        .release(
                            lock.org_id(),
                            lock.project(),
                            lock.namespace(),
                            lock.name(),
                            &agent_id,
                        )
                        .await;
                }
            }

            let watchers = WatcherStore::find_by_agent(&*self.container.store, &agent_id)
                .await
                .unwrap_or_default();
            for w in &watchers {
                if *w.project() == current_project {
                    let _ =
                        WatcherStore::delete(&*self.container.store, &w.task_id(), &agent_id).await;
                }
            }

            let reviews = ReviewStore::find_pending_for_agent(&*self.container.store, &agent_id)
                .await
                .unwrap_or_default();
            for mut r in reviews {
                if *r.project() == current_project {
                    r.unassign_reviewer();
                    let _ = ReviewStore::save(&*self.container.store, &mut r).await;
                }
            }
        }

        match self
            .container
            .agent_service
            .switch_context(
                &agent_id,
                &org,
                target_project.clone(),
                target_namespace.clone(),
            )
            .await
        {
            Ok(agent) => {
                let final_project = target_project.unwrap_or(current_project);
                self.set_session_project_and_namespace(final_project, target_namespace);
                Ok(to_json(&agent))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Create a task. Use parent_id to create a subtask. \
        Tasks with depends_on are auto-blocked until dependencies complete.")]
    async fn post_task(
        &self,
        Parameters(params): Parameters<PostTaskParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
            .await?;

        let priority = match params.priority.as_deref() {
            Some(p) => match p.parse::<Priority>() {
                Ok(pri) => pri,
                Err(e) => return Err(format!("invalid priority: {e}")),
            },
            None => Priority::default(),
        };

        let depends_on: Vec<TaskId> = params
            .depends_on
            .unwrap_or_default()
            .iter()
            .map(|s| parse_task_id(s))
            .collect::<Result<Vec<_>, _>>()?;

        let parent_id = match params.parent_id.as_deref() {
            Some(s) => Some(parse_task_id(s)?),
            None => None,
        };

        let is_blocked = !depends_on.is_empty();
        let task = match Task::new(
            org,
            project,
            namespace,
            parent_id,
            params.title,
            params.description,
            priority,
            params.assigned_roles.unwrap_or_default(),
            depends_on,
            self.get_session_agent(),
            is_blocked,
        ) {
            Ok(t) => t,
            Err(e) => return Err(e.to_string()),
        };

        let response = to_json(&task);
        match self.container.task_service.create(task).await {
            Ok(()) => Ok(response),
            Err(e) => Err(e.to_string()),
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
        let (agent_id, _, _, _) = self.require_session()?;

        let namespace = Some(
            self.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
                .await?,
        );

        let roles = match params.role {
            Some(r) => vec![r],
            None => match self.container.agent_service.get(&agent_id).await {
                Ok(agent) => agent.roles().to_vec(),
                Err(e) => return Err(format!("error fetching agent roles: {e}")),
            },
        };

        let claim = params.claim.unwrap_or(true);

        if claim {
            match self
                .container
                .task_service
                .get_next(&agent_id, &roles, namespace)
                .await
            {
                Ok(Some(task)) => {
                    let ctx = self
                        .container
                        .task_service
                        .get_with_context(&task.id())
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(to_json(&ctx))
                }
                Ok(None) => Ok("no tasks available".to_string()),
                Err(e) => Err(e.to_string()),
            }
        } else {
            match self
                .container
                .task_service
                .peek_next(&roles, namespace)
                .await
            {
                Ok(Some(task)) => {
                    let ctx = self
                        .container
                        .task_service
                        .get_with_context(&task.id())
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(to_json(&ctx))
                }
                Ok(None) => Ok("no tasks available".to_string()),
                Err(e) => Err(e.to_string()),
            }
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
        let _ = self.require_session()?;

        let session_project = self.get_session_project();
        let project = if let Some(p) = params.project {
            Some(parse_project(&p)?)
        } else {
            session_project
        };

        let namespace = Some(
            self.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
                .await?,
        );

        let status = params.status.as_deref().map(|s| match s {
            "pending" => Some(orchy_core::task::TaskStatus::Pending),
            "blocked" => Some(orchy_core::task::TaskStatus::Blocked),
            "claimed" => Some(orchy_core::task::TaskStatus::Claimed),
            "in_progress" => Some(orchy_core::task::TaskStatus::InProgress),
            "completed" => Some(orchy_core::task::TaskStatus::Completed),
            "failed" => Some(orchy_core::task::TaskStatus::Failed),
            "cancelled" => Some(orchy_core::task::TaskStatus::Cancelled),
            _ => None,
        });

        if params.status.is_some() && status == Some(None) {
            return Err("invalid status value".to_string());
        }

        let parent_id = params.parent_id.as_deref().map(parse_task_id).transpose()?;

        let filter = TaskFilter {
            project,
            namespace,
            status: status.flatten(),
            parent_id,
            ..Default::default()
        };

        self.container
            .task_service
            .list(filter)
            .await
            .map(|tasks| to_json(&tasks))
            .map_err(|e| e.to_string())
    }

    #[tool(description = "Claim a specific task for the session agent. \
        When start is true, moves claimed → in_progress in the same call.")]
    async fn claim_task(
        &self,
        Parameters(params): Parameters<ClaimTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;

        let mut task = match self.container.task_service.claim(&task_id, &agent_id).await {
            Ok(t) => t,
            Err(e) => return Err(e.to_string()),
        };

        if params.start.unwrap_or(false) {
            task = match self.container.task_service.start(&task_id, &agent_id).await {
                Ok(t) => t,
                Err(e) => return Err(e.to_string()),
            };
        }

        let ctx = self
            .container
            .task_service
            .get_with_context(&task.id())
            .await
            .map_err(|e| e.to_string())?;
        Ok(to_json(&ctx))
    }

    #[tool(description = "Start a claimed task (claimed → in_progress). \
        Must be claimed by you first. Returns task with full context.")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<StartTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;

        match self.container.task_service.start(&task_id, &agent_id).await {
            Ok(task) => {
                let ctx = self
                    .container
                    .task_service
                    .get_with_context(&task.id())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(to_json(&ctx))
            }
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;

        match self
            .container
            .task_service
            .complete(&task_id, params.summary)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Mark a task as failed with an optional reason.")]
    async fn fail_task(
        &self,
        Parameters(params): Parameters<FailTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;

        match self
            .container
            .task_service
            .fail(&task_id, params.reason)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;

        match self
            .container
            .task_service
            .cancel(&task_id, params.reason)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Update task title, description, and/or priority. \
        Must be pending, claimed, in_progress, or blocked.")]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;

        let priority = match params.priority.as_deref() {
            Some(p) => Some(
                p.parse::<Priority>()
                    .map_err(|e| format!("invalid priority: {e}"))?,
            ),
            None => None,
        };

        match self
            .container
            .task_service
            .update_details(&task_id, params.title, params.description, priority)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;

        match self.container.task_service.unblock_manual(&task_id).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;

        let agent_id = self.resolve_agent_id(&params.agent).await?;

        match self
            .container
            .task_service
            .assign(&task_id, &agent_id)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Send a message. Target: agent UUID, 'role:name' (all agents \
        with that role), or 'broadcast' (all agents except you)."
    )]
    async fn send_message(
        &self,
        Parameters(params): Parameters<SendMessageParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, _) = self.require_session()?;

        let target = match MessageTarget::parse(&params.to) {
            Ok(t) => t,
            Err(_) => match self.resolve_agent_id(&params.to).await {
                Ok(id) => MessageTarget::Agent(id),
                Err(_) => {
                    return Err(format!(
                        "invalid target: '{}' (not a UUID, role:name, broadcast, or known alias)",
                        params.to
                    ));
                }
            },
        };

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
            .await?;

        let reply_to = match params.reply_to {
            Some(s) => match s.parse::<MessageId>() {
                Ok(id) => Some(id),
                Err(e) => return Err(format!("invalid reply_to: {e}")),
            },
            None => None,
        };

        match self
            .container
            .message_service
            .send(SendMessage {
                org_id: org,
                project,
                namespace,
                from: agent_id,
                to: target,
                body: params.body,
                reply_to,
            })
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
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
            parse_project(&p)?
        } else {
            session_project
        };

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        match self
            .container
            .message_service
            .check(&agent_id, &org, &project, &namespace)
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List messages you have sent, with delivery and read status.")]
    async fn check_sent_messages(
        &self,
        Parameters(params): Parameters<CheckSentMessagesParams>,
    ) -> Result<String, String> {
        let (agent_id, org, session_project, _) = self.require_session()?;

        let project = if let Some(p) = params.project {
            parse_project(&p)?
        } else {
            session_project
        };

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        match self
            .container
            .message_service
            .sent(&agent_id, &org, &project, &namespace)
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Mark messages as read by their IDs for the session agent.")]
    async fn mark_read(
        &self,
        Parameters(params): Parameters<MarkReadParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;
        let ids: Vec<MessageId> = params
            .message_ids
            .iter()
            .map(|s| parse_message_id(s))
            .collect::<Result<Vec<_>, _>>()?;

        match self
            .container
            .message_service
            .mark_read(&agent_id, &ids)
            .await
        {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "List the full conversation thread for a given message ID. \
        Walks the reply_to chain to find the root, then returns all messages in \
        the thread in chronological order. Use limit to cap the number of messages \
        returned (most recent N). Does not require a registered session."
    )]
    async fn list_conversation(
        &self,
        Parameters(params): Parameters<ListConversationParams>,
    ) -> Result<String, String> {
        let message_id = parse_message_id(&params.message_id)?;

        let limit = params.limit.map(|n| n as usize);

        match self
            .container
            .message_service
            .thread(&message_id, limit)
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Add a note to a task. Notes are timestamped comments attached to the task."
    )]
    async fn add_task_note(
        &self,
        Parameters(params): Parameters<AddTaskNoteParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;

        match self
            .container
            .task_service
            .add_note(&task_id, Some(agent_id), params.body)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;

        let mut subtasks = Vec::new();
        for sp in params.subtasks {
            let priority = match sp.priority.as_deref() {
                Some(p) => match p.parse::<Priority>() {
                    Ok(pri) => pri,
                    Err(e) => return Err(format!("invalid priority: {e}")),
                },
                None => Priority::default(),
            };
            let depends_on: Vec<TaskId> = sp
                .depends_on
                .unwrap_or_default()
                .iter()
                .map(|s| parse_task_id(s))
                .collect::<Result<Vec<_>, _>>()?;
            subtasks.push(orchy_core::task::SubtaskDef {
                title: sp.title,
                description: sp.description,
                priority,
                assigned_roles: sp.assigned_roles.unwrap_or_default(),
                depends_on,
            });
        }

        match self
            .container
            .task_service
            .split_task(&task_id, subtasks, Some(agent_id))
            .await
        {
            Ok((parent, children)) => {
                let result = serde_json::json!({
                    "parent": parent,
                    "subtasks": children,
                });
                Ok(to_json(&result))
            }
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;

        let mut replacements = Vec::new();
        for sp in params.replacements {
            let priority = match sp.priority.as_deref() {
                Some(p) => match p.parse::<Priority>() {
                    Ok(pri) => pri,
                    Err(e) => return Err(format!("invalid priority: {e}")),
                },
                None => Priority::default(),
            };
            let depends_on: Vec<TaskId> = sp
                .depends_on
                .unwrap_or_default()
                .iter()
                .map(|s| parse_task_id(s))
                .collect::<Result<Vec<_>, _>>()?;
            replacements.push(orchy_core::task::SubtaskDef {
                title: sp.title,
                description: sp.description,
                priority,
                assigned_roles: sp.assigned_roles.unwrap_or_default(),
                depends_on,
            });
        }

        match self
            .container
            .task_service
            .replace_task(&task_id, params.reason, replacements, Some(agent_id))
            .await
        {
            Ok((original, new_tasks)) => {
                let result = serde_json::json!({
                    "cancelled": original,
                    "replacements": new_tasks,
                });
                Ok(to_json(&result))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Merge multiple tasks into one. Source tasks must be pending, \
        blocked, or claimed. They are cancelled and a new consolidated task is created \
        with the highest priority, combined roles, combined dependencies, and collected notes. \
        Children of source tasks are re-parented. Tasks depending on sources are updated."
    )]
    async fn merge_tasks(
        &self,
        Parameters(params): Parameters<MergeTasksParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let task_ids: Vec<TaskId> = params
            .task_ids
            .iter()
            .map(|s| parse_task_id(s))
            .collect::<Result<Vec<_>, _>>()?;

        match self
            .container
            .task_service
            .merge_tasks(&task_ids, params.title, params.description, Some(agent_id))
            .await
        {
            Ok((merged, cancelled)) => {
                let result = serde_json::json!({
                    "merged": merged,
                    "cancelled": cancelled,
                });
                Ok(to_json(&result))
            }
            Err(e) => Err(e.to_string()),
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
        let (agent_id, org, project, _) = self.require_session()?;

        let parent_id = parse_task_id(&params.task_id)?;
        let parent = self
            .container
            .task_service
            .get(&parent_id)
            .await
            .map_err(|e| e.to_string())?;

        let priority = match params.priority.as_deref() {
            Some(p) => match p.parse::<Priority>() {
                Ok(pri) => pri,
                Err(e) => return Err(format!("invalid priority: {e}")),
            },
            None => parent.priority(),
        };

        let task = match Task::new(
            org,
            project,
            parent.namespace().clone(),
            Some(parent_id),
            params.title,
            params.description,
            priority,
            params.assigned_roles.unwrap_or_default(),
            vec![],
            Some(agent_id.clone()),
            false,
        ) {
            Ok(t) => t,
            Err(e) => return Err(e.to_string()),
        };

        let response = to_json(&task);
        match self.container.task_service.create(task).await {
            Ok(()) => Ok(response),
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;
        let dep_id = parse_task_id(&params.dependency_id)?;

        match self
            .container
            .task_service
            .add_dependency(&task_id, &dep_id)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let task_id = parse_task_id(&params.task_id)?;
        let dep_id = parse_task_id(&params.dependency_id)?;

        match self
            .container
            .task_service
            .remove_dependency(&task_id, &dep_id)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let project = self
            .container
            .project_service
            .get_or_create(&org, &project_id)
            .await
            .map_err(|e| e.to_string())?;

        if !params.include_summary.unwrap_or(false) {
            return Ok(to_json(&project));
        }

        let agents = self
            .container
            .agent_service
            .list(&org)
            .await
            .map_err(|e| e.to_string())?;
        let project_agents: Vec<_> = agents
            .into_iter()
            .filter(|a| {
                *a.project() == project_id
                    && a.status() != orchy_core::agent::AgentStatus::Disconnected
            })
            .collect();

        let all_tasks = self
            .container
            .task_service
            .list(TaskFilter {
                project: Some(project_id.clone()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;

        let mut by_status = std::collections::HashMap::new();
        for task in &all_tasks {
            *by_status.entry(task.status().to_string()).or_insert(0u32) += 1;
        }

        let mut recent: Vec<_> = all_tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status(),
                    orchy_core::task::TaskStatus::Completed | orchy_core::task::TaskStatus::Failed
                )
            })
            .collect();
        recent.sort_by_key(|b| std::cmp::Reverse(b.updated_at()));
        recent.truncate(10);

        let recent_items: Vec<_> = recent
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id().to_string(),
                    "title": t.title(),
                    "status": t.status().to_string(),
                    "summary": t.result_summary(),
                })
            })
            .collect();

        let agent_id = self.get_session_agent();
        let mut my_workload_by_status: std::collections::HashMap<String, Vec<serde_json::Value>> =
            std::collections::HashMap::new();
        if let Some(ref aid) = agent_id {
            for task in &all_tasks {
                if task.assigned_to() == Some(aid) {
                    my_workload_by_status
                        .entry(task.status().to_string())
                        .or_default()
                        .push(serde_json::json!({
                            "id": task.id().to_string(),
                            "title": task.title(),
                            "priority": task.priority().to_string(),
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

        let project = self
            .container
            .project_service
            .get_or_create(&org, &project_id)
            .await
            .map_err(|e| e.to_string())?;

        if let Some(expected) = params.version {
            let updated = project.updated_at().timestamp() as u64;
            if expected != updated {
                return Err(format!(
                    "version mismatch: expected {}, got {}",
                    expected, updated
                ));
            }
        }

        let description = params
            .description
            .unwrap_or_else(|| project.description().to_string());

        match self
            .container
            .project_service
            .update_description(&org, &project_id, description)
            .await
        {
            Ok(project) => Ok(to_json(&project)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Set a metadata key-value pair on the current session's project.")]
    async fn set_project_metadata(
        &self,
        Parameters(params): Parameters<SetProjectMetadataParams>,
    ) -> Result<String, String> {
        let (_, org, project_id, _) = self.require_session()?;

        match self
            .container
            .project_service
            .set_metadata(&org, &project_id, params.key, params.value)
            .await
        {
            Ok(project) => Ok(to_json(&project)),
            Err(e) => Err(e.to_string()),
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
            parse_project(&p)?
        } else {
            session_project
        };

        match NamespaceStore::list(&*self.container.store, &org, &project).await {
            Ok(namespaces) => Ok(to_json(&namespaces)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Move a task to a different namespace within the same project.")]
    async fn move_task(
        &self,
        Parameters(params): Parameters<MoveTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;

        let namespace = self
            .resolve_namespace(Some(&params.new_namespace), NamespacePolicy::RegisterIfNew)
            .await?;

        self.container
            .task_service
            .move_task(&task_id, namespace)
            .await
            .map(|task| to_json(&task))
            .map_err(|e| e.to_string())
    }

    #[tool(
        description = "Get a comprehensive project overview: instructions, connected agents, \
        active tasks, and skills. Also available as HTTP GET /bootstrap/{project}."
    )]
    async fn get_project_overview(
        &self,
        Parameters(params): Parameters<GetProjectOverviewParams>,
    ) -> Result<String, String> {
        let project = if let Some(p) = params.project {
            parse_project(&p)?
        } else {
            self.get_session_project()
                .ok_or("no agent registered for this session; call register_agent first")?
        };

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
            .await?;

        let host = &self.container.config.server.host;
        let port = self.container.config.server.port;

        match crate::bootstrap::generate_bootstrap_prompt(
            &project,
            &namespace,
            host,
            port,
            &self.container.knowledge_service,
            &self.container.project_service,
            &self.container.agent_service,
            &self.container.task_service,
        )
        .await
        {
            Ok(prompt) => Ok(prompt),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Add a tag to a task.")]
    async fn tag_task(
        &self,
        Parameters(params): Parameters<TagTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self.container.task_service.tag(&task_id, params.tag).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Remove a tag from a task.")]
    async fn untag_task(
        &self,
        Parameters(params): Parameters<UntagTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self
            .container
            .task_service
            .untag(&task_id, &params.tag)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
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

        let ttl = params.ttl_secs.unwrap_or(300);

        match self
            .container
            .lock_service
            .acquire(org, project, namespace, params.name, agent_id, ttl)
            .await
        {
            Ok(lock) => Ok(to_json(&lock)),
            Err(e) => Err(e.to_string()),
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

        match self
            .container
            .lock_service
            .release(&org, &project, &namespace, &params.name, &agent_id)
            .await
        {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
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

        match self
            .container
            .lock_service
            .check(&org, &project, &namespace, &params.name)
            .await
        {
            Ok(Some(lock)) => Ok(to_json(&lock)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Release a claimed or in-progress task back to pending.")]
    async fn release_task(
        &self,
        Parameters(params): Parameters<ReleaseTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self.container.task_service.release(&task_id).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List all unique tags used across tasks. \
        Defaults to session namespace; pass namespace=/ to see all namespaces.")]
    async fn list_tags(
        &self,
        Parameters(params): Parameters<ListTagsParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = Some(
            self.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
                .await?,
        );

        let tasks = self
            .container
            .task_service
            .list(TaskFilter {
                project: Some(project),
                namespace,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;

        let mut tags: Vec<String> = tasks
            .iter()
            .flat_map(|t| t.tags().iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        tags.sort();

        Ok(to_json(&tags))
    }

    #[tool(description = "Get a task by its ID with full context (ancestors and children).")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<GetTaskParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self.container.task_service.get_with_context(&task_id).await {
            Ok(ctx) => Ok(to_json(&ctx)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Watch a task for status changes. You'll receive mailbox notifications \
        when the task is started, completed, failed, or has a dependency failure."
    )]
    async fn watch_task(
        &self,
        Parameters(params): Parameters<WatchTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, namespace) = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self
            .container
            .task_service
            .watch(&task_id, agent_id, org, project, namespace)
            .await
        {
            Ok(watcher) => Ok(to_json(&watcher)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Stop watching a task.")]
    async fn unwatch_task(
        &self,
        Parameters(params): Parameters<UnwatchTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self
            .container
            .task_service
            .unwatch(&task_id, &agent_id)
            .await
        {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Request a review for a task. Sends a notification to the target reviewer \
        (by agent ID or role). Use list_reviews to check status."
    )]
    async fn request_review(
        &self,
        Parameters(params): Parameters<RequestReviewParams>,
    ) -> Result<String, String> {
        let (agent_id, org, project, namespace) = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        let reviewer = match params.reviewer_agent.as_deref() {
            Some(s) => Some(self.resolve_agent_id(s).await?),
            None => None,
        };

        match self
            .container
            .task_service
            .request_review(RequestReviewCommand {
                task_id,
                org_id: org,
                project,
                namespace,
                requester: agent_id,
                reviewer,
                reviewer_role: params.reviewer_role,
            })
            .await
        {
            Ok(review) => Ok(to_json(&review)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Approve or reject a review request.")]
    async fn resolve_review(
        &self,
        Parameters(params): Parameters<ResolveReviewParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _, _) = self.require_session()?;

        let review_id = parse_review_id(&params.review_id)?;

        match self
            .container
            .task_service
            .resolve_review(&review_id, agent_id, params.approved, params.comments)
            .await
        {
            Ok(review) => Ok(to_json(&review)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List review requests for a task.")]
    async fn list_reviews(
        &self,
        Parameters(params): Parameters<ListReviewsParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let task_id = parse_task_id(&params.task_id)?;
        match self
            .container
            .task_service
            .list_reviews_for_task(&task_id)
            .await
        {
            Ok(reviews) => Ok(to_json(&reviews)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Get a single review request by ID.")]
    async fn get_review(
        &self,
        Parameters(params): Parameters<GetReviewParams>,
    ) -> Result<String, String> {
        let _ = self.require_session()?;

        let review_id = parse_review_id(&params.review_id)?;
        match self.container.task_service.get_review(&review_id).await {
            Ok(review) => Ok(to_json(&review)),
            Err(e) => Err(e.to_string()),
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
        let project = if let Some(p) = params.project {
            parse_project(&p)?
        } else {
            session_project
        };

        let since = match params.since.as_deref() {
            Some(s) => chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| format!("invalid timestamp: {e}"))?,
            None => chrono::Utc::now() - chrono::Duration::minutes(5),
        };

        let limit = params.limit.unwrap_or(50) as usize;

        let events = self
            .container
            .store
            .query_events(project.as_ref(), since, limit)
            .await
            .map_err(|e| e.to_string())?;

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
            "since": since.to_rfc3339(),
            "count": updates.len(),
            "events": updates,
        });

        Ok(to_json(&result))
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

        let kind: KnowledgeKind = params.kind.parse().map_err(|e: String| e)?;

        let metadata = knowledge_metadata_from_json_str(params.metadata.as_deref(), "metadata")?;
        let metadata_remove = params.metadata_remove.unwrap_or_default();

        let cmd = WriteKnowledge {
            org_id: org,
            project: Some(project),
            namespace,
            path: params.path,
            kind,
            title: params.title,
            content: params.content,
            tags: params.tags.unwrap_or_default(),
            expected_version: params.version.map(KnowledgeVersion::from),
            agent_id: self.get_session_agent(),
            metadata,
            metadata_remove,
        };

        match self.container.knowledge_service.write(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        match self
            .container
            .knowledge_service
            .patch_metadata(PatchKnowledgeMetadata {
                org,
                project: Some(project),
                namespace,
                path: params.path,
                set,
                remove,
                expected_version: params.version.map(KnowledgeVersion::from),
            })
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Read a knowledge entry by path. \
        Defaults to session namespace; pass namespace=/ for root.")]
    async fn read_knowledge(
        &self,
        Parameters(params): Parameters<ReadKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, session_project, _) = self.require_session()?;
        let project = if let Some(p) = params.project {
            parse_project(&p)?
        } else {
            session_project
        };

        let namespace = self
            .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
            .await?;

        match self
            .container
            .knowledge_service
            .read(&org, Some(&project), &namespace, &params.path)
            .await
        {
            Ok(Some(entry)) => Ok(to_json(&entry)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
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
        let explicit_project = params.project.as_deref().map(parse_project).transpose()?;
        let namespace = match params.namespace.as_deref() {
            Some(_) => Some(
                self.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
                    .await?,
            ),
            None => None,
        };

        let kind = match params.kind.as_deref() {
            Some(t) => Some(t.parse::<KnowledgeKind>().map_err(|e: String| e)?),
            None => None,
        };

        let agent_id = match params.agent.as_deref() {
            Some(s) => Some(self.resolve_agent_id(s).await?),
            None => None,
        };

        let project = if explicit_project.is_some() {
            explicit_project
        } else if namespace.is_none() {
            self.get_session_project()
        } else {
            None
        };

        let filter = KnowledgeFilter {
            project,
            namespace,
            kind,
            tag: params.tag,
            path_prefix: params.path_prefix,
            agent_id,
            ..Default::default()
        };

        match self.container.knowledge_service.list(filter).await {
            Ok(entries) => Ok(to_json(&entries)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Search knowledge entries by semantic similarity. \
        Defaults to session namespace; pass namespace=/ to search all namespaces.")]
    async fn search_knowledge(
        &self,
        Parameters(params): Parameters<SearchKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, _, _) = self.require_session()?;

        let namespace = Some(
            self.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
                .await?,
        );

        let limit = params.limit.unwrap_or(10) as usize;

        let mut entries = match self
            .container
            .knowledge_service
            .search(&org, &params.query, namespace.as_ref(), limit)
            .await
        {
            Ok(e) => e,
            Err(e) => return Err(e.to_string()),
        };

        if let Some(k) = params.kind.as_deref() {
            let kind: KnowledgeKind = k.parse().map_err(|e: String| e)?;
            entries.retain(|e| e.kind() == kind);
        }

        if let Some(p) = params.project {
            let project = parse_project(&p)?;
            entries.retain(|e| e.project().map(|ep| *ep == project).unwrap_or(false));
        }

        Ok(to_json(&entries))
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

        let entry = self
            .container
            .knowledge_service
            .read(&org, Some(&project), &namespace, &params.path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry not found: {}", params.path))?;

        match self.container.knowledge_service.delete(&entry.id()).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
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

        let kind: KnowledgeKind = params.kind.parse().map_err(|e: String| e)?;

        let separator = params.separator.as_deref().unwrap_or("\n");

        let meta = optional_knowledge_metadata(params.metadata, "metadata")?;
        let meta_remove = params.metadata_remove;

        match self
            .container
            .knowledge_service
            .append(
                &org,
                Some(&project),
                &namespace,
                &params.path,
                kind,
                params.value,
                separator,
                self.get_session_agent(),
                meta,
                meta_remove,
            )
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        let entry = self
            .container
            .knowledge_service
            .read(&org, Some(&project), &namespace, &params.path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry not found: {}", params.path))?;

        let new_namespace = self
            .resolve_namespace(Some(&params.new_namespace), NamespacePolicy::RegisterIfNew)
            .await?;

        let meta = optional_knowledge_metadata(params.metadata, "metadata")?;
        let meta_remove = params.metadata_remove;

        match self
            .container
            .knowledge_service
            .move_entry(&entry.id(), new_namespace, meta, meta_remove)
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        let entry = self
            .container
            .knowledge_service
            .read(&org, Some(&project), &namespace, &params.path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry not found: {}", params.path))?;

        let meta = optional_knowledge_metadata(params.metadata, "metadata")?;
        let meta_remove = params.metadata_remove;

        match self
            .container
            .knowledge_service
            .rename(&entry.id(), params.new_path, meta, meta_remove)
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        let new_kind: KnowledgeKind = params.kind.parse().map_err(|e: String| e)?;

        let expected = params.version.map(KnowledgeVersion::from);
        let meta = optional_knowledge_metadata(params.metadata, "metadata")?;
        let meta_remove = params.metadata_remove;

        match self
            .container
            .knowledge_service
            .change_kind(
                &org,
                Some(&project),
                &namespace,
                &params.path,
                new_kind,
                expected,
                meta,
                meta_remove,
            )
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        let entry = self
            .container
            .knowledge_service
            .read(&org, Some(&project), &namespace, &params.path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry not found: {}", params.path))?;

        let meta = optional_knowledge_metadata(params.metadata, "metadata")?;
        let meta_remove = params.metadata_remove;

        match self
            .container
            .knowledge_service
            .tag(&entry.id(), params.tag, meta, meta_remove)
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        let entry = self
            .container
            .knowledge_service
            .read(&org, Some(&project), &namespace, &params.path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry not found: {}", params.path))?;

        let meta = optional_knowledge_metadata(params.metadata, "metadata")?;
        let meta_remove = params.metadata_remove;

        match self
            .container
            .knowledge_service
            .untag(&entry.id(), &params.tag, meta, meta_remove)
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Import a knowledge entry from a linked project.")]
    async fn import_knowledge(
        &self,
        Parameters(params): Parameters<ImportKnowledgeParams>,
    ) -> Result<String, String> {
        let (_, org, project, _) = self.require_session()?;

        let source_project = parse_project(&params.source_project)?;

        let source_namespace = match params.source_namespace.as_deref() {
            Some(s) => parse_namespace(&format!("/{s}"))?,
            None => Namespace::root(),
        };

        let source_entry = self
            .container
            .knowledge_service
            .read(&org, Some(&source_project), &source_namespace, &params.path)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("entry not found in source: {}", params.path))?;

        let namespace = self
            .resolve_namespace(None, NamespacePolicy::RegisterIfNew)
            .await?;

        let mut md = source_entry.metadata().clone();
        if let Some(keys) = params.metadata_remove {
            for k in keys {
                md.remove(&k);
            }
        }
        if let Some(overlay) = optional_knowledge_metadata(params.metadata, "metadata")? {
            md.extend(overlay);
        }

        let cmd = WriteKnowledge {
            org_id: org,
            project: Some(project),
            namespace,
            path: source_entry.path().to_string(),
            kind: source_entry.kind(),
            title: source_entry.title().to_string(),
            content: source_entry.content().to_string(),
            tags: source_entry.tags().to_vec(),
            expected_version: None,
            agent_id: self.get_session_agent(),
            metadata: md,
            metadata_remove: vec![],
        };

        match self.container.knowledge_service.write(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
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

        let skills = self
            .container
            .knowledge_service
            .list_skills(&org, &project, &namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let prompts = skills
            .into_iter()
            .map(|s| Prompt::new(s.title().to_string(), Some(s.title().to_string()), None))
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

        let skills = self
            .container
            .knowledge_service
            .list_skills(&org, &project, &namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let entry = skills
            .into_iter()
            .find(|s| s.title() == request.name)
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("skill '{}' not found", request.name), None)
            })?;

        let mut result = GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            entry.content().to_string(),
        )]);
        result.description = Some(entry.title().to_string());
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
