use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;

use orchy_core::entities::{
    CreateMessage, CreateSnapshot, CreateTask, MemoryFilter, RegisterAgent, TaskFilter, WriteMemory,
};
use orchy_core::value_objects::{
    AgentId, MessageId, MessageTarget, Namespace, Priority, TaskId, Version,
};

use super::handler::OrchyHandler;

// ---------------------------------------------------------------------------
// Parameter structs
// ---------------------------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
struct RegisterAgentParams {
    roles: Vec<String>,
    description: String,
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListAgentsParams {}

#[derive(Deserialize, schemars::JsonSchema)]
struct PostTaskParams {
    namespace: String,
    title: String,
    description: String,
    priority: Option<String>,
    assigned_roles: Option<Vec<String>>,
    depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetNextTaskParams {
    namespace: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListTasksParams {
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
struct FailTaskParams {
    task_id: String,
    reason: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WriteMemoryParams {
    namespace: String,
    key: String,
    value: String,
    version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ReadMemoryParams {
    namespace: String,
    key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListMemoryParams {
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchMemoryParams {
    query: String,
    namespace: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteMemoryParams {
    namespace: String,
    key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SendMessageParams {
    to: String,
    body: String,
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CheckMailboxParams {
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MarkReadParams {
    message_ids: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SaveContextParams {
    summary: String,
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
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchContextsParams {
    query: String,
    namespace: Option<String>,
    agent_id: Option<String>,
    limit: Option<u32>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_namespace(s: &str) -> Result<Namespace, String> {
    Namespace::try_from(s.to_string())
}

fn parse_task_id(s: &str) -> Result<TaskId, String> {
    s.parse::<TaskId>().map_err(|e| format!("invalid task_id: {e}"))
}

fn parse_agent_id(s: &str) -> Result<AgentId, String> {
    s.parse::<AgentId>().map_err(|e| format!("invalid agent_id: {e}"))
}

fn parse_message_id(s: &str) -> Result<MessageId, String> {
    s.parse::<MessageId>().map_err(|e| format!("invalid message_id: {e}"))
}

fn to_json<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|e| format!("serialization error: {e}"))
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl OrchyHandler {
    // === Agent tools ===

    #[tool(description = "Register this session as an agent. Returns the assigned agent ID.")]
    async fn register_agent(
        &self,
        Parameters(params): Parameters<RegisterAgentParams>,
    ) -> String {
        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let cmd = RegisterAgent {
            namespace,
            roles: params.roles,
            description: params.description,
            metadata: HashMap::new(),
        };

        match self.container.agent_service.register(cmd).await {
            Ok(agent) => {
                self.set_session_agent(agent.id);
                to_json(&agent)
            }
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List all connected agents.")]
    async fn list_agents(&self, Parameters(_params): Parameters<ListAgentsParams>) -> String {
        match self.container.agent_service.list().await {
            Ok(agents) => to_json(&agents),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Send a heartbeat for the session agent to signal liveness.")]
    async fn heartbeat(&self) -> String {
        let agent_id = match self.require_session_agent() {
            Ok(id) => id,
            Err(e) => return e,
        };

        match self.container.agent_service.heartbeat(&agent_id).await {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    // === Task tools ===

    #[tool(description = "Create a new task in the given namespace.")]
    async fn post_task(&self, Parameters(params): Parameters<PostTaskParams>) -> String {
        let namespace = match parse_namespace(&params.namespace) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let priority = match params.priority.as_deref() {
            Some(p) => match p.parse::<Priority>() {
                Ok(pri) => pri,
                Err(e) => return format!("invalid priority: {e}"),
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
            Err(e) => return e,
        };

        let cmd = CreateTask {
            namespace,
            title: params.title,
            description: params.description,
            priority,
            assigned_roles: params.assigned_roles.unwrap_or_default(),
            depends_on,
            created_by: self.get_session_agent(),
        };

        match self.container.task_service.create(cmd).await {
            Ok(task) => to_json(&task),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Get the next available task for the session agent, optionally filtered by namespace and role.")]
    async fn get_next_task(&self, Parameters(params): Parameters<GetNextTaskParams>) -> String {
        let agent_id = match self.require_session_agent() {
            Ok(id) => id,
            Err(e) => return e,
        };

        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        // If a role filter is specified, search with just that role.
        // Otherwise use all roles the agent registered with (unavailable here, pass empty).
        let roles = match params.role {
            Some(r) => vec![r],
            None => {
                // Fetch agent to get roles
                match self.container.agent_service.get(&agent_id).await {
                    Ok(agent) => agent.roles,
                    Err(e) => return format!("error fetching agent roles: {e}"),
                }
            }
        };

        match self
            .container
            .task_service
            .get_next(&agent_id, &roles, namespace)
            .await
        {
            Ok(Some(task)) => to_json(&task),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List tasks, optionally filtered by namespace and status.")]
    async fn list_tasks(&self, Parameters(params): Parameters<ListTasksParams>) -> String {
        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let status = params.status.as_deref().map(|s| match s {
            "pending" => Some(orchy_core::value_objects::TaskStatus::Pending),
            "blocked" => Some(orchy_core::value_objects::TaskStatus::Blocked),
            "claimed" => Some(orchy_core::value_objects::TaskStatus::Claimed),
            "in_progress" => Some(orchy_core::value_objects::TaskStatus::InProgress),
            "completed" => Some(orchy_core::value_objects::TaskStatus::Completed),
            "failed" => Some(orchy_core::value_objects::TaskStatus::Failed),
            _ => None,
        });

        // If status string was provided but didn't match, it's an error
        if params.status.is_some() && status == Some(None) {
            return "invalid status value".to_string();
        }

        let filter = TaskFilter {
            namespace,
            status: status.flatten(),
            ..Default::default()
        };

        match self.container.task_service.list(filter).await {
            Ok(tasks) => to_json(&tasks),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Claim a specific task for the session agent.")]
    async fn claim_task(&self, Parameters(params): Parameters<ClaimTaskParams>) -> String {
        let agent_id = match self.require_session_agent() {
            Ok(id) => id,
            Err(e) => return e,
        };

        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return e,
        };

        match self.container.task_service.claim(&task_id, &agent_id).await {
            Ok(task) => to_json(&task),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Mark a task as completed with an optional summary.")]
    async fn complete_task(&self, Parameters(params): Parameters<CompleteTaskParams>) -> String {
        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return e,
        };

        match self
            .container
            .task_service
            .complete(&task_id, params.summary)
            .await
        {
            Ok(task) => to_json(&task),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Mark a task as failed with an optional reason.")]
    async fn fail_task(&self, Parameters(params): Parameters<FailTaskParams>) -> String {
        let task_id = match parse_task_id(&params.task_id) {
            Ok(id) => id,
            Err(e) => return e,
        };

        match self
            .container
            .task_service
            .fail(&task_id, params.reason)
            .await
        {
            Ok(task) => to_json(&task),
            Err(e) => format!("error: {e}"),
        }
    }

    // === Memory tools ===

    #[tool(description = "Write a key-value entry to shared memory in the given namespace.")]
    async fn write_memory(&self, Parameters(params): Parameters<WriteMemoryParams>) -> String {
        let namespace = match parse_namespace(&params.namespace) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let cmd = WriteMemory {
            namespace,
            key: params.key,
            value: params.value,
            expected_version: params.version.map(Version::from),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: self.get_session_agent(),
        };

        match self.container.memory_service.write(cmd).await {
            Ok(entry) => to_json(&entry),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Read a memory entry by namespace and key.")]
    async fn read_memory(&self, Parameters(params): Parameters<ReadMemoryParams>) -> String {
        let namespace = match parse_namespace(&params.namespace) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self.container.memory_service.read(&namespace, &params.key).await {
            Ok(Some(entry)) => to_json(&entry),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List memory entries, optionally filtered by namespace.")]
    async fn list_memory(&self, Parameters(params): Parameters<ListMemoryParams>) -> String {
        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let filter = MemoryFilter { namespace };

        match self.container.memory_service.list(filter).await {
            Ok(entries) => to_json(&entries),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Search memory entries by semantic similarity. Requires embeddings to be configured.")]
    async fn search_memory(&self, Parameters(params): Parameters<SearchMemoryParams>) -> String {
        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let limit = params.limit.unwrap_or(10) as usize;

        match self
            .container
            .memory_service
            .search(&params.query, namespace.as_ref(), limit)
            .await
        {
            Ok(entries) => to_json(&entries),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Delete a memory entry by namespace and key.")]
    async fn delete_memory(&self, Parameters(params): Parameters<DeleteMemoryParams>) -> String {
        let namespace = match parse_namespace(&params.namespace) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self.container.memory_service.delete(&namespace, &params.key).await {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    // === Message tools ===

    #[tool(description = "Send a message to another agent (by ID), a role (role:name), or broadcast.")]
    async fn send_message(&self, Parameters(params): Parameters<SendMessageParams>) -> String {
        let agent_id = match self.require_session_agent() {
            Ok(id) => id,
            Err(e) => return e,
        };

        let target = match MessageTarget::parse(&params.to) {
            Ok(t) => t,
            Err(e) => return format!("invalid target: {e}"),
        };

        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let cmd = CreateMessage {
            namespace,
            from: agent_id,
            to: target,
            body: params.body,
        };

        match self.container.message_service.send(cmd).await {
            Ok(messages) => to_json(&messages),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Check the mailbox for pending messages for the session agent.")]
    async fn check_mailbox(&self, Parameters(params): Parameters<CheckMailboxParams>) -> String {
        let agent_id = match self.require_session_agent() {
            Ok(id) => id,
            Err(e) => return e,
        };

        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        match self
            .container
            .message_service
            .check(&agent_id, namespace.as_ref())
            .await
        {
            Ok(messages) => to_json(&messages),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Mark messages as read by their IDs.")]
    async fn mark_read(&self, Parameters(params): Parameters<MarkReadParams>) -> String {
        let ids: Vec<MessageId> = match params
            .message_ids
            .iter()
            .map(|s| parse_message_id(s))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(ids) => ids,
            Err(e) => return e,
        };

        match self.container.message_service.mark_read(&ids).await {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    // === Context tools ===

    #[tool(description = "Save a context snapshot for the session agent with a summary.")]
    async fn save_context(&self, Parameters(params): Parameters<SaveContextParams>) -> String {
        let agent_id = match self.require_session_agent() {
            Ok(id) => id,
            Err(e) => return e,
        };

        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let metadata: HashMap<String, String> = match params.metadata.as_deref() {
            Some(json_str) => match serde_json::from_str(json_str) {
                Ok(m) => m,
                Err(e) => return format!("invalid metadata JSON: {e}"),
            },
            None => HashMap::new(),
        };

        let cmd = CreateSnapshot {
            agent_id,
            namespace,
            summary: params.summary,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata,
        };

        match self.container.context_service.save(cmd).await {
            Ok(snapshot) => to_json(&snapshot),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Load the most recent context snapshot for an agent (defaults to session agent).")]
    async fn load_context(&self, Parameters(params): Parameters<LoadContextParams>) -> String {
        let agent_id = match params.agent_id.as_deref() {
            Some(id_str) => match parse_agent_id(id_str) {
                Ok(id) => id,
                Err(e) => return e,
            },
            None => match self.require_session_agent() {
                Ok(id) => id,
                Err(e) => return e,
            },
        };

        match self.container.context_service.load(&agent_id).await {
            Ok(Some(snapshot)) => to_json(&snapshot),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List context snapshots, optionally filtered by agent and namespace.")]
    async fn list_contexts(&self, Parameters(params): Parameters<ListContextsParams>) -> String {
        let agent_id = match params.agent_id.as_deref().map(parse_agent_id) {
            Some(Ok(id)) => Some(id),
            Some(Err(e)) => return e,
            None => None,
        };

        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        match self
            .container
            .context_service
            .list(agent_id.as_ref(), namespace.as_ref())
            .await
        {
            Ok(snapshots) => to_json(&snapshots),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Search context snapshots by semantic similarity.")]
    async fn search_contexts(
        &self,
        Parameters(params): Parameters<SearchContextsParams>,
    ) -> String {
        let namespace = match params.namespace.as_deref().map(parse_namespace) {
            Some(Ok(ns)) => Some(ns),
            Some(Err(e)) => return e,
            None => None,
        };

        let agent_id = match params.agent_id.as_deref().map(parse_agent_id) {
            Some(Ok(id)) => Some(id),
            Some(Err(e)) => return e,
            None => None,
        };

        let limit = params.limit.unwrap_or(10) as usize;

        match self
            .container
            .context_service
            .search(&params.query, namespace.as_ref(), agent_id.as_ref(), limit)
            .await
        {
            Ok(snapshots) => to_json(&snapshots),
            Err(e) => format!("error: {e}"),
        }
    }
}

#[tool_handler(instructions = "orchy - multi-agent orchestration server")]
impl ServerHandler for OrchyHandler {}
