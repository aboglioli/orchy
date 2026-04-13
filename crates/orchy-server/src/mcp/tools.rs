use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, PaginatedRequestParams, Prompt, PromptMessage,
    PromptMessageRole, ServerCapabilities, ServerInfo,
};

type ListPromptsResult = rmcp::model::ListPromptsResult;
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, schemars, tool, tool_router};
use serde::Deserialize;

use orchy_core::agent::RegisterAgent;
use orchy_core::memory::{MemoryFilter, Version, WriteMemory};
use orchy_core::message::{MessageId, MessageTarget};
use orchy_core::namespace::{Namespace, NamespaceStore};
use orchy_core::skill::{SkillFilter, WriteSkill};
use orchy_core::task::{Priority, Task, TaskFilter, TaskId};

use super::handler::{OrchyHandler, parse_namespace, parse_project};

#[derive(Deserialize, schemars::JsonSchema)]
struct RegisterAgentParams {
    /// Project identifier (e.g. "my-project").
    project: String,
    /// Scope within the project (e.g. "backend" or "backend/auth"). Optional.
    namespace: Option<String>,
    /// Agent capabilities (e.g. ["coder", "reviewer"]). If empty or omitted, orchy assigns roles based on pending task demand.
    roles: Option<Vec<String>>,
    description: String,
    /// Resume an existing agent by its ID.
    agent_id: Option<String>,
    /// Create a new agent as a child of the given parent agent ID.
    parent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ChangeRolesParams {
    roles: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListAgentsParams {}

#[derive(Deserialize, schemars::JsonSchema)]
struct PostTaskParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    /// Parent task ID for creating subtasks. Optional.
    parent_id: Option<String>,
    title: String,
    description: String,
    priority: Option<String>,
    assigned_roles: Option<Vec<String>>,
    depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetNextTaskParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListTasksParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ClaimTaskParams {
    task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CompleteTaskParams {
    task_id: String,
    summary: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct StartTaskParams {
    task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct FailTaskParams {
    task_id: String,
    reason: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AssignTaskParams {
    task_id: String,
    agent_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WriteMemoryParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    key: String,
    value: String,
    version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ReadMemoryParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListMemoryParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchMemoryParams {
    query: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteMemoryParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SendMessageParams {
    to: String,
    body: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    /// ID of a message this is replying to.
    reply_to: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CheckMailboxParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MarkReadParams {
    message_ids: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CheckSentMessagesParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListConversationParams {
    /// ID of any message in the conversation thread.
    message_id: String,
    /// Maximum number of messages to return (most recent). Optional.
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SaveContextParams {
    summary: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    /// JSON string of metadata key-value pairs
    metadata: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct LoadContextParams {
    agent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListContextsParams {
    agent_id: Option<String>,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchContextsParams {
    query: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    agent_id: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WriteSkillParams {
    /// Skill name (e.g. "commit-conventions", "architecture", "coding-style").
    name: String,
    /// Short description shown when listing skills.
    description: String,
    /// Full skill content — the instructions/prompt text agents will receive.
    content: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ReadSkillParams {
    name: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListSkillsParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
    /// If true, include inherited skills from parent namespaces. Defaults to false.
    inherited: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteSkillParams {
    name: String,
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SplitTaskParams {
    /// ID of the parent task to split.
    task_id: String,
    /// Subtask definitions.
    subtasks: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SubtaskParam {
    title: String,
    description: String,
    priority: Option<String>,
    assigned_roles: Option<Vec<String>>,
    depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ReplaceTaskParams {
    /// ID of the task to replace.
    task_id: String,
    /// Reason for replacing the task.
    reason: Option<String>,
    /// Replacement task definitions.
    replacements: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AddDependencyParams {
    task_id: String,
    dependency_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct RemoveDependencyParams {
    task_id: String,
    dependency_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MoveTaskParams {
    task_id: String,
    /// New scope to move the task to (e.g. "backend/auth").
    new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MoveMemoryParams {
    /// Scope within the project (e.g. 'backend'). Optional — defaults to session namespace.
    namespace: Option<String>,
    key: String,
    /// New scope to move the entry to (e.g. "backend/auth").
    new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MoveSkillParams {
    /// Scope within the project (e.g. 'backend'). Optional — defaults to session namespace.
    namespace: Option<String>,
    name: String,
    /// New scope to move the skill to (e.g. "backend/auth").
    new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetBootstrapPromptParams {
    /// Scope within the project (e.g. 'backend'). Optional.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AddTaskNoteParams {
    task_id: String,
    body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MoveAgentParams {
    /// New scope within the project to move to (e.g. "backend/auth").
    namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetProjectParams {}

#[derive(Deserialize, schemars::JsonSchema)]
struct UpdateProjectParams {
    description: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AddProjectNoteParams {
    body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListNamespacesParams {}

fn parse_task_id(s: &str) -> Result<TaskId, String> {
    s.parse::<TaskId>()
        .map_err(|e| format!("invalid task_id: {e}"))
}

fn parse_agent_id(s: &str) -> Result<orchy_core::agent::AgentId, String> {
    s.parse::<orchy_core::agent::AgentId>()
        .map_err(|e| format!("invalid agent_id: {e}"))
}

fn parse_message_id(s: &str) -> Result<MessageId, String> {
    s.parse::<MessageId>()
        .map_err(|e| format!("invalid message_id: {e}"))
}

fn to_json<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|e| format!("serialization error: {e}"))
}

#[tool_router]
impl OrchyHandler {
    #[tool(
        description = "Register this session as an agent within a project namespace. \
        All subsequent tool calls will be scoped to this project. \
        If roles is empty, orchy assigns roles based on pending task demand. \
        Use agent_id to resume a previous agent (same identity, comes back online). \
        Use parent_id to create a new agent inheriting from a parent (lineage tracking)."
    )]
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
        description = "Disconnect the session agent. Releases all claimed tasks back to pending. \
        Use this when your session is ending."
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

    #[tool(
        description = "Create a new task. Namespace defaults to session namespace; \
        if provided, the project prefix must match."
    )]
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
        let task = Task::new(
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
        );

        let response = to_json(&task);
        match self.container.task_service.create(task).await {
            Ok(()) => Ok(response),
            Err(e) => Err(e.to_string()),
        }
    }

    #[tool(
        description = "Get the next available task for the session agent, optionally filtered \
        by namespace and role. Returns the task with full context: parent task \
        (if this is a subtask) and children (if this task was split)."
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

    #[tool(
        description = "Start working on a claimed task (transitions from claimed to in_progress). \
        You must claim a task before starting it, and start it before completing it. \
        Workflow: pending → claimed → in_progress → completed/failed."
    )]
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
        description = "Assign a task to an agent. If the task is already assigned, \
        it will be reassigned to the new agent."
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
        description = "Write a key-value entry to shared memory. Namespace defaults to \
        session namespace; if provided, the project prefix must match."
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

    #[tool(description = "Read a memory entry by key. Namespace defaults to session namespace.")]
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

    #[tool(description = "Delete a memory entry by key. Namespace defaults to session namespace.")]
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
        description = "Send a message to another agent (by ID), a role (role:name), or \
        broadcast. Namespace defaults to session namespace."
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
        to session namespace."
    )]
    async fn search_contexts(
        &self,
        Parameters(params): Parameters<SearchContextsParams>,
    ) -> Result<String, String> {
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
        description = "Write a project skill — shared instructions/conventions that all \
        agents in this project will receive. Skills are identified by namespace + name. \
        Writing to an existing name updates it. Namespace defaults to session namespace."
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
        description = "List skills for the project. If inherited=true, includes skills \
        from parent namespaces with more specific ones taking precedence (requires namespace). \
        If namespace is omitted, returns all skills in the project."
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
            self.container
                .skill_service
                .list_with_inherited(&project, &namespace)
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
        description = "Generate a full bootstrap prompt for this project. Contains all \
        orchy instructions, coordination patterns, and project skills in a single text block. \
        Useful for agents that don't support MCP server instructions natively — copy-paste \
        this into their system prompt. Also available as HTTP GET /bootstrap/<namespace>."
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
}

const INSTRUCTIONS: &str = "\
orchy — multi-agent coordination server.

You are part of a coordinated multi-agent system. orchy provides shared \
infrastructure: a task board, shared memory, messaging, skills, and \
project context. You bring the intelligence; orchy enforces the rules.

## On Session Start

1. Call `register_agent` with your project, roles, and description.
2. Call `list_skills(inherited=true)` and follow the project conventions.
3. Call `get_project` to read the project description and notes.
4. Call `get_next_task` to claim work, or `check_mailbox` for messages.
5. Call `heartbeat` every ~30s to signal liveness.

## Project & Namespace

Each agent belongs to a project (e.g. `my-project`). Resources are \
organized in namespaces within the project: `/` is root, `/backend`, \
`/backend/auth` are scopes. Namespace is optional for reading — omit \
it to see all project resources. Write operations default to your \
current namespace. Use `move_agent` to switch namespaces. Use \
`list_namespaces` to discover available scopes.

## Coordination

- Claim tasks before working. Complete them with a summary when done.
- Split large tasks with `split_task` — parent auto-completes when subtasks finish.
- Replace tasks with `replace_task` to cancel and create new ones.
- Manage dependencies with `add_dependency` and `remove_dependency`.
- Use shared memory to store decisions and context for other agents.
- Use messages to coordinate with teammates. Reply with `reply_to`.
- Check delivery status with `check_sent_messages`.
- Browse conversation threads with `list_conversation`.
- Save context before your session ends for continuity.
- Use `list_skills(inherited=true)` to get project conventions.
- Register without roles to let orchy assign roles based on task demand.

## Bootstrap Prompt

If your client doesn't support MCP instructions, call `get_bootstrap_prompt` \
to get a full copy-pasteable prompt with all orchy instructions and project skills.";

impl ServerHandler for OrchyHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_instructions(INSTRUCTIONS.to_string())
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        self.touch_heartbeat();
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: Self::tool_router().list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let (project, namespace) = match (self.get_session_project(), self.get_session_namespace())
        {
            (Some(p), Some(ns)) => (p, ns),
            _ => {
                return Ok(ListPromptsResult {
                    prompts: vec![],
                    meta: None,
                    next_cursor: None,
                });
            }
        };

        let skills = self
            .container
            .skill_service
            .list_with_inherited(&project, &namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let prompts = skills
            .into_iter()
            .map(|s| {
                Prompt::new(
                    s.name().to_string(),
                    Some(s.description().to_string()),
                    None,
                )
            })
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
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        let project = self
            .get_session_project()
            .ok_or_else(|| ErrorData::internal_error("no session project", None))?;
        let namespace = self
            .get_session_namespace()
            .ok_or_else(|| ErrorData::internal_error("no session namespace", None))?;

        let skill = self
            .container
            .skill_service
            .read(&project, &namespace, &request.name)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let skill = match skill {
            Some(s) => s,
            None => {
                let inherited = self
                    .container
                    .skill_service
                    .list_with_inherited(&project, &namespace)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                inherited
                    .into_iter()
                    .find(|s| s.name() == request.name)
                    .ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("skill '{}' not found", request.name),
                            None,
                        )
                    })?
            }
        };

        let mut result = GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            skill.content().to_string(),
        )]);
        result.description = Some(skill.description().to_string());
        Ok(result)
    }
}
