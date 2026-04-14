use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};

use orchy_core::agent::RegisterAgent;
use orchy_core::document::{DocumentFilter, WriteDocument};
use orchy_core::memory::MemoryStore;
use orchy_core::memory::{MemoryFilter, Version, WriteMemory};
use orchy_core::message::{MessageId, MessageTarget};
use orchy_core::namespace::{Namespace, NamespaceStore};
use orchy_core::project_link::SharedResourceType;
use orchy_core::skill::{SkillFilter, WriteSkill};
use orchy_core::task::{Priority, Task, TaskFilter, TaskId};

use super::handler::{
    OrchyHandler, parse_agent_id, parse_message_id, parse_namespace, parse_project, parse_task_id,
    to_json,
};
use super::params::*;

#[tool_router]
impl OrchyHandler {
    #[tool(description = "Register as an agent. Required before any other tool. \
        Roles are optional — orchy assigns them from pending task demand if omitted. \
        Use agent_id to resume a previous session. Use parent_id for agent lineage.")]
    async fn register_agent(
        &self,
        Parameters(params): Parameters<RegisterAgentParams>,
    ) -> Result<String, String> {
        let project = parse_project(&params.project)?;

        let namespace = match params.namespace.as_deref() {
            Some(s) if !s.is_empty() => parse_namespace(&format!("/{s}"))?,
            _ => Namespace::root(),
        };

        if let Some(ref id_str) = params.agent_id {
            let agent_id = parse_agent_id(id_str)?;
            let _ = NamespaceStore::register(&*self.container.store, &project, &namespace).await;

            match self
                .container
                .agent_service
                .resume(
                    &agent_id,
                    namespace.clone(),
                    params.roles.clone().unwrap_or_default(),
                    params.description.clone(),
                )
                .await
            {
                Ok(agent) => {
                    self.set_session(agent.id(), project, namespace);
                    return Ok(to_json(&agent));
                }
                Err(e) => return Err(e.to_string()),
            }
        }

        let _ = NamespaceStore::register(&*self.container.store, &project, &namespace).await;

        let parent_id = params.parent_id.map(|s| parse_agent_id(&s)).transpose()?;

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
            project: project.clone(),
            namespace: namespace.clone(),
            roles,
            description: params.description,
            parent_id,
            metadata: HashMap::new(),
        };

        match self.container.agent_service.register(cmd).await {
            Ok(agent) => {
                self.set_session(agent.id(), project, namespace);
                Ok(to_json(&agent))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List connected agents in the current project.")]
    async fn list_agents(
        &self,
        Parameters(_params): Parameters<ListAgentsParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        match self.container.agent_service.list().await {
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
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        if let Err(e) = self
            .container
            .task_service
            .release_agent_tasks(&agent_id)
            .await
        {
            return Err(e.to_string());
        }

        match self.container.agent_service.disconnect(&agent_id).await {
            Ok(()) => Ok("disconnected".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Move the session agent to a new namespace within the same project. \
        Updates both the agent record and the session scope."
    )]
    async fn move_agent(
        &self,
        Parameters(params): Parameters<MoveAgentParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self
            .build_and_register_namespace(Some(&params.namespace))
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .agent_service
            .move_to(&agent_id, namespace.clone())
            .await
        {
            Ok(agent) => {
                self.set_session_namespace(namespace);
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
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let priority = match params.priority.as_deref() {
            Some(p) => match p.parse::<Priority>() {
                Ok(pri) => pri,
                Err(e) => return Err(format!("invalid priority: {e}")),
            },
            None => Priority::default(),
        };

        let depends_on: Vec<TaskId> = match params
            .depends_on
            .unwrap_or_default()
            .iter()
            .map(|s| parse_task_id(s))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(ids) => ids,
            Err(e) => return Err(e),
        };

        let parent_id = match params.parent_id.as_deref() {
            Some(s) => Some(parse_task_id(s)?),
            None => None,
        };

        let is_blocked = !depends_on.is_empty();
        let task = match Task::new(
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

    #[tool(
        description = "Claim the next available task matching your roles and return it. \
        Returns full context: ancestor chain (if subtask) and children (if split). \
        Skips tasks with incomplete dependencies."
    )]
    async fn get_next_task(
        &self,
        Parameters(params): Parameters<GetNextTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let roles = match params.role {
            Some(r) => vec![r],
            None => match self.container.agent_service.get(&agent_id).await {
                Ok(agent) => agent.roles().to_vec(),
                Err(e) => return Err(format!("error fetching agent roles: {e}")),
            },
        };

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
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "List tasks, optionally filtered by namespace and status. \
        If namespace is omitted, returns all tasks in the project."
    )]
    async fn list_tasks(
        &self,
        Parameters(params): Parameters<ListTasksParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

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

        let filter = TaskFilter {
            project: if namespace.is_none() {
                self.get_session_project()
            } else {
                None
            },
            namespace,
            status: status.flatten(),
            ..Default::default()
        };

        match self.container.task_service.list(filter).await {
            Ok(tasks) => Ok(to_json(&tasks)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Claim a specific task for the session agent.")]
    async fn claim_task(
        &self,
        Parameters(params): Parameters<ClaimTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        match self.container.task_service.claim(&task_id, &agent_id).await {
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

    #[tool(description = "Start a claimed task (claimed → in_progress). \
        Must be claimed by you first. Returns task with full context.")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<StartTaskParams>,
    ) -> Result<String, String> {
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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

    #[tool(description = "Mark a task as completed with an optional summary.")]
    async fn complete_task(
        &self,
        Parameters(params): Parameters<CompleteTaskParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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
        description = "Assign a claimed or in-progress task to a different agent. \
        The task must already be claimed."
    )]
    async fn assign_task(
        &self,
        Parameters(params): Parameters<AssignTaskParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        let agent_id = match parse_agent_id(&params.agent_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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
        description = "Write a key-value entry to shared memory. Use version param for \
        optimistic concurrency (fails if entry was modified since you read it)."
    )]
    async fn write_memory(
        &self,
        Parameters(params): Parameters<WriteMemoryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let cmd = WriteMemory {
            project,
            namespace,
            key: params.key,
            value: params.value,
            expected_version: params.version.map(Version::from),
            written_by: self.get_session_agent(),
        };

        match self.container.memory_service.write(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Read a memory entry by key. Namespace defaults to root.")]
    async fn read_memory(
        &self,
        Parameters(params): Parameters<ReadMemoryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        match self
            .container
            .memory_service
            .read(&project, &namespace, &params.key)
            .await
        {
            Ok(Some(entry)) => Ok(to_json(&entry)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "List memory entries. If namespace is omitted, returns all entries \
        in the project."
    )]
    async fn list_memory(
        &self,
        Parameters(params): Parameters<ListMemoryParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let filter = MemoryFilter {
            project: if namespace.is_none() {
                self.get_session_project()
            } else {
                None
            },
            namespace,
        };

        match self.container.memory_service.list(filter).await {
            Ok(entries) => Ok(to_json(&entries)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Search memory entries by semantic similarity. If namespace is omitted, \
        searches all entries in the project."
    )]
    async fn search_memory(
        &self,
        Parameters(params): Parameters<SearchMemoryParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let limit = params.limit.unwrap_or(10) as usize;

        match self
            .container
            .memory_service
            .search(&params.query, namespace.as_ref(), limit)
            .await
        {
            Ok(entries) => Ok(to_json(&entries)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Delete a memory entry by key. Namespace defaults to root.")]
    async fn delete_memory(
        &self,
        Parameters(params): Parameters<DeleteMemoryParams>,
    ) -> Result<String, String> {
        let project = self.get_session_project().ok_or("no agent registered")?;
        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .memory_service
            .delete(&project, &namespace, &params.key)
            .await
        {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Append text to an existing memory entry. Creates the entry if it \
        doesn't exist. Uses optimistic concurrency to avoid lost updates."
    )]
    async fn append_memory(
        &self,
        Parameters(params): Parameters<AppendMemoryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let separator = params.separator.as_deref().unwrap_or("\n");

        let existing = self
            .container
            .memory_service
            .read(&project, &namespace, &params.key)
            .await
            .map_err(|e| e.to_string())?;

        let (new_value, expected_version) = match existing {
            Some(entry) => {
                let combined = format!("{}{}{}", entry.value(), separator, params.value);
                (combined, Some(entry.version()))
            }
            None => (params.value, None),
        };

        let cmd = WriteMemory {
            project,
            namespace,
            key: params.key,
            value: new_value,
            expected_version,
            written_by: self.get_session_agent(),
        };

        match self.container.memory_service.write(cmd).await {
            Ok(entry) => Ok(to_json(&entry)),
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
        let (agent_id, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let target = match MessageTarget::parse(&params.to) {
            Ok(t) => t,
            Err(e) => return Err(format!("invalid target: {e}")),
        };

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

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
            .send(project, namespace, agent_id, target, params.body, reply_to)
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Check the mailbox for pending messages. Namespace defaults to session \
        namespace."
    )]
    async fn check_mailbox(
        &self,
        Parameters(params): Parameters<CheckMailboxParams>,
    ) -> Result<String, String> {
        let (agent_id, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        match self
            .container
            .message_service
            .check(&agent_id, &project, &namespace)
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Mark messages as read by their IDs.")]
    async fn mark_read(
        &self,
        Parameters(params): Parameters<MarkReadParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let ids: Vec<MessageId> = match params
            .message_ids
            .iter()
            .map(|s| parse_message_id(s))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(ids) => ids,
            Err(e) => return Err(e),
        };

        match self.container.message_service.mark_read(&ids).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Check the delivery/read status of messages you have sent. \
        Namespace defaults to root."
    )]
    async fn check_sent_messages(
        &self,
        Parameters(params): Parameters<CheckSentMessagesParams>,
    ) -> Result<String, String> {
        let (agent_id, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        match self
            .container
            .message_service
            .sent(&agent_id, &project, &namespace)
            .await
        {
            Ok(messages) => Ok(to_json(&messages)),
            Err(e) => Err(e.to_string()),
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let message_id = match parse_message_id(&params.message_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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
        description = "Save a context snapshot for the session agent. Namespace defaults to \
        session namespace."
    )]
    async fn save_context(
        &self,
        Parameters(params): Parameters<SaveContextParams>,
    ) -> Result<String, String> {
        let (agent_id, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let metadata: HashMap<String, String> = match params.metadata.as_deref() {
            Some(json_str) => match serde_json::from_str(json_str) {
                Ok(m) => m,
                Err(e) => return Err(format!("invalid metadata JSON: {e}")),
            },
            None => HashMap::new(),
        };

        match self
            .container
            .context_service
            .save(project, agent_id, namespace, params.summary, metadata)
            .await
        {
            Ok(snapshot) => Ok(to_json(&snapshot)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Load the most recent context snapshot for an agent (defaults to \
        session agent)."
    )]
    async fn load_context(
        &self,
        Parameters(params): Parameters<LoadContextParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let agent_id = match params.agent_id.as_deref() {
            Some(id_str) => match parse_agent_id(id_str) {
                Ok(id) => id,
                Err(e) => return Err(e),
            },
            None => match self.require_session() {
                Ok((id, _, _)) => id,
                Err(e) => return Err(e),
            },
        };

        match self.container.context_service.load(&agent_id).await {
            Ok(Some(snapshot)) => Ok(to_json(&snapshot)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List context snapshots. Namespace defaults to session namespace.")]
    async fn list_contexts(
        &self,
        Parameters(params): Parameters<ListContextsParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let agent_id = match params.agent_id.as_deref().map(parse_agent_id) {
            Some(Ok(id)) => Some(id),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        match self
            .container
            .context_service
            .list(agent_id.as_ref(), &namespace)
            .await
        {
            Ok(snapshots) => Ok(to_json(&snapshots)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Search context snapshots by semantic similarity. Namespace defaults \
        to root."
    )]
    async fn search_contexts(
        &self,
        Parameters(params): Parameters<SearchContextsParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        let agent_id = match params.agent_id.as_deref().map(parse_agent_id) {
            Some(Ok(id)) => Some(id),
            Some(Err(e)) => return Err(e),
            None => None,
        };

        let limit = params.limit.unwrap_or(10) as usize;

        match self
            .container
            .context_service
            .search(&params.query, &namespace, agent_id.as_ref(), limit)
            .await
        {
            Ok(snapshots) => Ok(to_json(&snapshots)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Write a project skill (shared instructions/conventions). \
        All agents receive these via list_skills. Writing to an existing name updates it."
    )]
    async fn write_skill(
        &self,
        Parameters(params): Parameters<WriteSkillParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let cmd = WriteSkill {
            project,
            namespace,
            name: params.name,
            description: params.description,
            content: params.content,
            written_by: self.get_session_agent(),
        };

        match self.container.skill_service.write(cmd).await {
            Ok(skill) => Ok(to_json(&skill)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Read a specific skill by name. Namespace defaults to session namespace.")]
    async fn read_skill(
        &self,
        Parameters(params): Parameters<ReadSkillParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        match self
            .container
            .skill_service
            .read(&project, &namespace, &params.name)
            .await
        {
            Ok(Some(skill)) => Ok(to_json(&skill)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "List skills. Use inherited=true to include skills from parent \
        namespaces and linked projects (child overrides parent on name collision, \
        local overrides linked)."
    )]
    async fn list_skills(
        &self,
        Parameters(params): Parameters<ListSkillsParams>,
    ) -> Result<String, String> {
        let result = if params.inherited.unwrap_or(false) {
            let project = self
                .get_session_project()
                .ok_or("no agent registered for this session; call register_agent first")?;
            let namespace = match params.namespace.as_deref() {
                Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
                None => Namespace::root(),
            };
            let linked = self
                .container
                .project_link_service
                .linked_projects(&project, SharedResourceType::Skills)
                .await
                .unwrap_or_default();
            self.container
                .skill_service
                .list_with_inherited_and_linked(&project, &namespace, &linked)
                .await
        } else {
            let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
                Ok(ns) => ns,
                Err(e) => return Err(e),
            };
            self.container
                .skill_service
                .list(SkillFilter {
                    project: if namespace.is_none() {
                        self.get_session_project()
                    } else {
                        None
                    },
                    namespace,
                })
                .await
        };

        match result {
            Ok(skills) => Ok(to_json(&skills)),
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        let author = self.get_session_agent();

        match self
            .container
            .task_service
            .add_note(&task_id, author, params.body)
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        let created_by = self.get_session_agent();

        let mut subtasks = Vec::new();
        for sp in params.subtasks {
            let priority = match sp.priority.as_deref() {
                Some(p) => match p.parse::<Priority>() {
                    Ok(pri) => pri,
                    Err(e) => return Err(format!("invalid priority: {e}")),
                },
                None => Priority::default(),
            };
            let depends_on: Vec<TaskId> = match sp
                .depends_on
                .unwrap_or_default()
                .iter()
                .map(|s| parse_task_id(s))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(ids) => ids,
                Err(e) => return Err(e),
            };
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
            .split_task(&task_id, subtasks, created_by)
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        let created_by = self.get_session_agent();

        let mut replacements = Vec::new();
        for sp in params.replacements {
            let priority = match sp.priority.as_deref() {
                Some(p) => match p.parse::<Priority>() {
                    Ok(pri) => pri,
                    Err(e) => return Err(format!("invalid priority: {e}")),
                },
                None => Priority::default(),
            };
            let depends_on: Vec<TaskId> = match sp
                .depends_on
                .unwrap_or_default()
                .iter()
                .map(|s| parse_task_id(s))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(ids) => ids,
                Err(e) => return Err(e),
            };
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
            .replace_task(&task_id, params.reason, replacements, created_by)
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_ids: Vec<TaskId> = match params
            .task_ids
            .iter()
            .map(|s| parse_task_id(s))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(ids) => ids,
            Err(e) => return Err(e),
        };

        match self
            .container
            .task_service
            .merge_tasks(
                &task_ids,
                params.title,
                params.description,
                self.get_session_agent(),
            )
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

    #[tool(description = "List direct children (subtasks) of a task.")]
    async fn list_subtasks(
        &self,
        Parameters(params): Parameters<ListSubtasksParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = parse_task_id(&params.task_id)?;

        match self
            .container
            .task_service
            .list(TaskFilter {
                parent_id: Some(task_id),
                ..Default::default()
            })
            .await
        {
            Ok(tasks) => Ok(to_json(&tasks)),
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
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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
            project,
            parent.namespace().clone(),
            Some(parent_id),
            params.title,
            params.description,
            priority,
            params.assigned_roles.unwrap_or_default(),
            vec![],
            self.get_session_agent(),
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };
        let dep_id = match parse_task_id(&params.dependency_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };
        let dep_id = match parse_task_id(&params.dependency_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

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

    #[tool(description = "Get the project metadata for the current session's project.")]
    async fn get_project(
        &self,
        Parameters(_params): Parameters<GetProjectParams>,
    ) -> Result<String, String> {
        let project_id = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        match self
            .container
            .project_service
            .get_or_create(&project_id)
            .await
        {
            Ok(project) => Ok(to_json(&project)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Update the project description for the current session's project.")]
    async fn update_project(
        &self,
        Parameters(params): Parameters<UpdateProjectParams>,
    ) -> Result<String, String> {
        let project_id = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        match self
            .container
            .project_service
            .update_description(&project_id, params.description)
            .await
        {
            Ok(project) => Ok(to_json(&project)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Add a note to the current session's project.")]
    async fn add_project_note(
        &self,
        Parameters(params): Parameters<AddProjectNoteParams>,
    ) -> Result<String, String> {
        let project_id = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let author = self.get_session_agent();

        match self
            .container
            .project_service
            .add_note(&project_id, author, params.body)
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
        Parameters(_params): Parameters<ListNamespacesParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        match NamespaceStore::list(&*self.container.store, &project).await {
            Ok(namespaces) => Ok(to_json(&namespaces)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Get an aggregated project overview: agent count, task counts by status, \
        and recent completions."
    )]
    async fn get_project_summary(
        &self,
        Parameters(_params): Parameters<GetProjectSummaryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let agents = self
            .container
            .agent_service
            .list()
            .await
            .map_err(|e| e.to_string())?;
        let project_agents: Vec<_> = agents
            .into_iter()
            .filter(|a| *a.project() == project)
            .collect();

        let all_tasks = self
            .container
            .task_service
            .list(TaskFilter {
                project: Some(project.clone()),
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
        recent.sort_by(|a, b| b.updated_at().cmp(&a.updated_at()));
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

        let result = serde_json::json!({
            "agents_online": project_agents.len(),
            "tasks_by_status": by_status,
            "total_tasks": all_tasks.len(),
            "recent_completions": recent_items,
        });

        Ok(to_json(&result))
    }

    #[tool(description = "Get tasks assigned to an agent grouped by status. \
        Defaults to the current agent.")]
    async fn get_agent_workload(
        &self,
        Parameters(params): Parameters<GetAgentWorkloadParams>,
    ) -> Result<String, String> {
        let (session_agent, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let agent_id = match params.agent_id.as_deref() {
            Some(s) => parse_agent_id(s)?,
            None => session_agent,
        };

        let tasks = self
            .container
            .task_service
            .list(TaskFilter {
                assigned_to: Some(agent_id),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;

        let mut by_status = std::collections::HashMap::new();
        for task in &tasks {
            by_status
                .entry(task.status().to_string())
                .or_insert_with(Vec::new)
                .push(serde_json::json!({
                    "id": task.id().to_string(),
                    "title": task.title(),
                    "priority": task.priority().to_string(),
                }));
        }

        let result = serde_json::json!({
            "agent_id": agent_id.to_string(),
            "total_tasks": tasks.len(),
            "by_status": by_status,
        });

        Ok(to_json(&result))
    }

    #[tool(description = "Move a task to a different namespace within the same project.")]
    async fn move_task(
        &self,
        Parameters(params): Parameters<MoveTaskParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return Err(e),
        };

        let namespace = match self
            .build_and_register_namespace(Some(&params.new_namespace))
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .task_service
            .move_task(&task_id, namespace)
            .await
        {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Move a memory entry to a different namespace within the same project. \
        Source namespace defaults to session namespace."
    )]
    async fn move_memory(
        &self,
        Parameters(params): Parameters<MoveMemoryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let new_namespace = match self
            .build_and_register_namespace(Some(&params.new_namespace))
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .memory_service
            .move_entry(&project, &namespace, &params.key, new_namespace)
            .await
        {
            Ok(entry) => Ok(to_json(&entry)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Move a skill to a different namespace within the same project. \
        Source namespace defaults to session namespace."
    )]
    async fn move_skill(
        &self,
        Parameters(params): Parameters<MoveSkillParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let new_namespace = match self
            .build_and_register_namespace(Some(&params.new_namespace))
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .skill_service
            .move_skill(&project, &namespace, &params.name, new_namespace)
            .await
        {
            Ok(skill) => Ok(to_json(&skill)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Delete a skill by name. Namespace defaults to session namespace.")]
    async fn delete_skill(
        &self,
        Parameters(params): Parameters<DeleteSkillParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .skill_service
            .delete(&project, &namespace, &params.name)
            .await
        {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Generate a full bootstrap prompt with all orchy instructions, \
        project skills, connected agents, and active tasks. For clients that don't \
        support MCP instructions natively."
    )]
    async fn get_bootstrap_prompt(
        &self,
        Parameters(params): Parameters<GetBootstrapPromptParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match params.namespace.as_deref() {
            Some(s) => self.build_namespace(Some(s)).map_err(|e| e.to_string())?,
            None => Namespace::root(),
        };

        let host = &self.container.config.server.host;
        let port = self.container.config.server.port;

        match crate::bootstrap::generate_bootstrap_prompt(
            &project,
            &namespace,
            host,
            port,
            &self.container.skill_service,
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

    #[tool(
        description = "Link another project as a resource source. Linked skills appear \
        in list_skills(inherited: true). Resource types: 'skills', 'memory'."
    )]
    async fn link_project(
        &self,
        Parameters(params): Parameters<LinkProjectParams>,
    ) -> Result<String, String> {
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let source = parse_project(&params.source_project)?;

        let resource_types: Vec<SharedResourceType> = match params
            .resource_types
            .iter()
            .map(|s| s.parse::<SharedResourceType>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(types) => types,
            Err(e) => return Err(e),
        };

        match self
            .container
            .project_link_service
            .link(source, project, resource_types)
            .await
        {
            Ok(link) => Ok(to_json(&link)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Remove a project link.")]
    async fn unlink_project(
        &self,
        Parameters(params): Parameters<UnlinkProjectParams>,
    ) -> Result<String, String> {
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let source = parse_project(&params.source_project)?;

        match self
            .container
            .project_link_service
            .unlink(&source, &project)
            .await
        {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List all project links for the current project.")]
    async fn list_project_links(
        &self,
        Parameters(_params): Parameters<ListProjectLinksParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        match self
            .container
            .project_link_service
            .list_links(&project)
            .await
        {
            Ok(links) => Ok(to_json(&links)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Copy a skill from another project into the current project. \
        One-time copy, not a live link."
    )]
    async fn import_skill(
        &self,
        Parameters(params): Parameters<ImportSkillParams>,
    ) -> Result<String, String> {
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let source_project = parse_project(&params.source_project)?;
        let source_ns = match params.source_namespace.as_deref() {
            Some(s) if !s.is_empty() => parse_namespace(&format!("/{s}"))?,
            _ => Namespace::root(),
        };

        let skill = match self
            .container
            .skill_service
            .read(&source_project, &source_ns, &params.name)
            .await
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                return Err(format!(
                    "skill '{}' not found in project {}",
                    params.name, source_project
                ));
            }
            Err(e) => return Err(e.to_string()),
        };

        let namespace = match self.build_and_register_namespace(None).await {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let cmd = WriteSkill {
            project,
            namespace,
            name: skill.name().to_string(),
            description: skill.description().to_string(),
            content: skill.content().to_string(),
            written_by: self.get_session_agent(),
        };

        match self.container.skill_service.write(cmd).await {
            Ok(imported) => Ok(to_json(&imported)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Copy a memory entry from another project into the current project. \
        One-time copy, not a live link."
    )]
    async fn import_memory(
        &self,
        Parameters(params): Parameters<ImportMemoryParams>,
    ) -> Result<String, String> {
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let source_project = parse_project(&params.source_project)?;
        let source_ns = match params.source_namespace.as_deref() {
            Some(s) if !s.is_empty() => parse_namespace(&format!("/{s}"))?,
            _ => Namespace::root(),
        };

        let entry = match self
            .container
            .memory_service
            .read(&source_project, &source_ns, &params.key)
            .await
        {
            Ok(Some(e)) => e,
            Ok(None) => {
                return Err(format!(
                    "memory '{}' not found in project {}",
                    params.key, source_project
                ));
            }
            Err(e) => return Err(e.to_string()),
        };

        let namespace = match self.build_and_register_namespace(None).await {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let cmd = WriteMemory {
            project,
            namespace,
            key: entry.key().to_string(),
            value: entry.value().to_string(),
            expected_version: None,
            written_by: self.get_session_agent(),
        };

        match self.container.memory_service.write(cmd).await {
            Ok(imported) => Ok(to_json(&imported)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Add a tag to a task.")]
    async fn tag_task(
        &self,
        Parameters(params): Parameters<TagTaskParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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

    #[tool(description = "Lock a memory entry to make it read-only.")]
    async fn lock_memory(
        &self,
        Parameters(params): Parameters<LockMemoryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let mut entry = match self
            .container
            .memory_service
            .read(&project, &namespace, &params.key)
            .await
        {
            Ok(Some(e)) => e,
            Ok(None) => return Err(format!("memory '{}' not found", params.key)),
            Err(e) => return Err(e.to_string()),
        };

        entry.lock();
        self.container
            .store
            .save(&mut entry)
            .await
            .map_err(|e| e.to_string())?;
        Ok(to_json(&entry))
    }

    #[tool(description = "Unlock a memory entry to make it writable again.")]
    async fn unlock_memory(
        &self,
        Parameters(params): Parameters<UnlockMemoryParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let mut entry = match self
            .container
            .memory_service
            .read(&project, &namespace, &params.key)
            .await
        {
            Ok(Some(e)) => e,
            Ok(None) => return Err(format!("memory '{}' not found", params.key)),
            Err(e) => return Err(e.to_string()),
        };

        entry.unlock();
        self.container
            .store
            .save(&mut entry)
            .await
            .map_err(|e| e.to_string())?;
        Ok(to_json(&entry))
    }

    #[tool(
        description = "Acquire a named distributed lock. Fails if held by another agent. \
        Locks auto-expire after ttl_secs (default 300)."
    )]
    async fn lock_resource(
        &self,
        Parameters(params): Parameters<LockResourceParams>,
    ) -> Result<String, String> {
        let (agent_id, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let ttl = params.ttl_secs.unwrap_or(300);

        match self
            .container
            .lock_service
            .acquire(project, namespace, params.name, agent_id, ttl)
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
        let (agent_id, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .lock_service
            .release(&project, &namespace, &params.name, &agent_id)
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
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .lock_service
            .check(&project, &namespace, &params.name)
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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = parse_task_id(&params.task_id)?;
        match self.container.task_service.release(&task_id).await {
            Ok(task) => Ok(to_json(&task)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List all unique tags used across tasks in the project.")]
    async fn list_tags(
        &self,
        Parameters(params): Parameters<ListTagsParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

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

    #[tool(
        description = "Create or update a document. Documents are markdown knowledge artifacts \
        for specs, architecture decisions, and shared analysis. Use version for optimistic concurrency."
    )]
    async fn write_document(
        &self,
        Parameters(params): Parameters<WriteDocumentParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self
            .build_and_register_namespace(params.namespace.as_deref())
            .await
        {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let cmd = WriteDocument {
            project,
            namespace,
            path: params.path,
            title: params.title,
            content: params.content,
            tags: params.tags.unwrap_or_default(),
            expected_version: params.version.map(Version::from),
            written_by: self.get_session_agent(),
        };

        match self.container.document_service.write(cmd).await {
            Ok(doc) => Ok(to_json(&doc)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Read a document by its path.")]
    async fn read_document(
        &self,
        Parameters(params): Parameters<ReadDocumentParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        match self
            .container
            .document_service
            .read_by_path(&project, &namespace, &params.path)
            .await
        {
            Ok(Some(doc)) => Ok(to_json(&doc)),
            Ok(None) => Ok("null".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "List documents with optional filters by namespace, tag, or path prefix.")]
    async fn list_documents(
        &self,
        Parameters(params): Parameters<ListDocumentsParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let filter = DocumentFilter {
            project: if namespace.is_none() {
                self.get_session_project()
            } else {
                None
            },
            namespace,
            tag: params.tag,
            path_prefix: params.path_prefix,
        };

        match self.container.document_service.list(filter).await {
            Ok(docs) => Ok(to_json(&docs)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Search documents by semantic similarity.")]
    async fn search_documents(
        &self,
        Parameters(params): Parameters<SearchDocumentsParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let namespace = match self.build_optional_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let limit = params.limit.unwrap_or(10) as usize;

        match self
            .container
            .document_service
            .search(&params.query, namespace.as_ref(), limit)
            .await
        {
            Ok(docs) => Ok(to_json(&docs)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Delete a document by path.")]
    async fn delete_document(
        &self,
        Parameters(params): Parameters<DeleteDocumentParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let doc = match self
            .container
            .document_service
            .read_by_path(&project, &namespace, &params.path)
            .await
        {
            Ok(Some(d)) => d,
            Ok(None) => return Err(format!("document '{}' not found", params.path)),
            Err(e) => return Err(e.to_string()),
        };

        match self.container.document_service.delete(&doc.id()).await {
            Ok(()) => Ok("ok".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Move a document to a different namespace.")]
    async fn move_document(
        &self,
        Parameters(params): Parameters<MoveDocumentParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let new_namespace = parse_namespace(&format!("/{}", params.new_namespace))?;

        let doc = match self
            .container
            .document_service
            .read_by_path(&project, &namespace, &params.path)
            .await
        {
            Ok(Some(d)) => d,
            Ok(None) => return Err(format!("document '{}' not found", params.path)),
            Err(e) => return Err(e.to_string()),
        };

        match self
            .container
            .document_service
            .move_doc(&doc.id(), new_namespace)
            .await
        {
            Ok(doc) => Ok(to_json(&doc)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Rename a document (change its path).")]
    async fn rename_document(
        &self,
        Parameters(params): Parameters<RenameDocumentParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let doc = match self
            .container
            .document_service
            .read_by_path(&project, &namespace, &params.path)
            .await
        {
            Ok(Some(d)) => d,
            Ok(None) => return Err(format!("document '{}' not found", params.path)),
            Err(e) => return Err(e.to_string()),
        };

        match self
            .container
            .document_service
            .rename(&doc.id(), params.new_path)
            .await
        {
            Ok(doc) => Ok(to_json(&doc)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Add a tag to a document.")]
    async fn tag_document(
        &self,
        Parameters(params): Parameters<TagDocumentParams>,
    ) -> Result<String, String> {
        let project = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        let namespace = match self.build_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return Err(e),
        };

        let doc = match self
            .container
            .document_service
            .read_by_path(&project, &namespace, &params.path)
            .await
        {
            Ok(Some(d)) => d,
            Ok(None) => return Err(format!("document '{}' not found", params.path)),
            Err(e) => return Err(e.to_string()),
        };

        match self
            .container
            .document_service
            .tag(&doc.id(), params.tag)
            .await
        {
            Ok(doc) => Ok(to_json(&doc)),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(description = "Get a task by its ID with full context (ancestors and children).")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<GetTaskParams>,
    ) -> Result<String, String> {
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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
        let (agent_id, project, namespace) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = parse_task_id(&params.task_id)?;
        match self
            .container
            .task_service
            .watch(&task_id, agent_id, project, namespace)
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
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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
        let (agent_id, project, namespace) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let task_id = parse_task_id(&params.task_id)?;
        let reviewer = match params.reviewer_agent.as_deref() {
            Some(s) => Some(parse_agent_id(s)?),
            None => None,
        };

        match self
            .container
            .task_service
            .request_review(
                &task_id,
                project,
                namespace,
                agent_id,
                reviewer,
                params.reviewer_role,
            )
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
        let (agent_id, _, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

        let review_id = params
            .review_id
            .parse()
            .map_err(|e| format!("invalid review_id: {e}"))?;

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
        let _ = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
        };

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

    #[tool(
        description = "Poll for recent events in the project since a timestamp. \
        Returns domain events (task changes, messages, document updates, etc). \
        Use alongside check_mailbox for full reactivity."
    )]
    async fn poll_updates(
        &self,
        Parameters(params): Parameters<PollUpdatesParams>,
    ) -> Result<String, String> {
        let (_, project, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return Err(e),
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
}
