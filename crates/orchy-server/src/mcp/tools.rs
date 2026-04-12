use std::collections::HashMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, PaginatedRequestParams, Prompt, PromptMessage,
    PromptMessageRole, ServerCapabilities, ServerInfo,
};

type ListPromptsResult = rmcp::model::ListPromptsResult;
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, schemars, tool, tool_router, ServerHandler};
use serde::Deserialize;

use orchy_core::entities::{
    CreateMessage, CreateSnapshot, CreateTask, MemoryFilter, RegisterAgent, SkillFilter,
    TaskFilter, WriteMemory, WriteSkill,
};
use orchy_core::value_objects::{MessageId, MessageTarget, Priority, TaskId, Version};

use super::handler::{parse_namespace, OrchyHandler};

#[derive(Deserialize, schemars::JsonSchema)]
struct RegisterAgentParams {
    /// Project namespace (first segment is the project identifier, e.g. "my-project"
    /// or "my-project/backend"). All tools in this session will be scoped to this
    /// project. Sub-scopes can be added later per tool call.
    namespace: String,
    roles: Vec<String>,
    description: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListAgentsParams {}

#[derive(Deserialize, schemars::JsonSchema)]
struct PostTaskParams {
    /// Namespace for the task. Defaults to session namespace. If provided, the first
    /// segment (project) must match the session's project.
    namespace: Option<String>,
    title: String,
    description: String,
    priority: Option<String>,
    assigned_roles: Option<Vec<String>>,
    depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetNextTaskParams {
    /// Namespace filter. Defaults to session namespace.
    namespace: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListTasksParams {
    /// Namespace filter. Defaults to session namespace.
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
    /// Namespace for the entry. Defaults to session namespace.
    namespace: Option<String>,
    key: String,
    value: String,
    version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ReadMemoryParams {
    /// Namespace for the entry. Defaults to session namespace.
    namespace: Option<String>,
    key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListMemoryParams {
    /// Namespace filter. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchMemoryParams {
    query: String,
    /// Namespace filter. Defaults to session namespace.
    namespace: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteMemoryParams {
    /// Namespace for the entry. Defaults to session namespace.
    namespace: Option<String>,
    key: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SendMessageParams {
    to: String,
    body: String,
    /// Namespace for the message. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CheckMailboxParams {
    /// Namespace filter. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct MarkReadParams {
    message_ids: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SaveContextParams {
    summary: String,
    /// Namespace for the snapshot. Defaults to session namespace.
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
    /// Namespace filter. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchContextsParams {
    query: String,
    /// Namespace filter. Defaults to session namespace.
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
    /// Namespace for the skill. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ReadSkillParams {
    name: String,
    /// Namespace. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListSkillsParams {
    /// Namespace filter. Defaults to session namespace.
    namespace: Option<String>,
    /// If true, include inherited skills from parent namespaces. Defaults to false.
    inherited: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteSkillParams {
    name: String,
    /// Namespace. Defaults to session namespace.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct GetBootstrapPromptParams {
    /// Namespace. Defaults to session namespace.
    namespace: Option<String>,
}

fn parse_task_id(s: &str) -> Result<TaskId, String> {
    s.parse::<TaskId>().map_err(|e| format!("invalid task_id: {e}"))
}

fn parse_agent_id(s: &str) -> Result<orchy_core::value_objects::AgentId, String> {
    s.parse::<orchy_core::value_objects::AgentId>()
        .map_err(|e| format!("invalid agent_id: {e}"))
}

fn parse_message_id(s: &str) -> Result<MessageId, String> {
    s.parse::<MessageId>().map_err(|e| format!("invalid message_id: {e}"))
}

fn to_json<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|e| format!("serialization error: {e}"))
}

#[tool_router]
impl OrchyHandler {
    #[tool(description = "Register this session as an agent within a project namespace. \
        The namespace must start with the project identifier (e.g. 'my-project' or \
        'my-project/backend'). All subsequent tool calls will be scoped to this project. \
        Sub-scopes can be provided per call, but the project prefix is always enforced.")]
    async fn register_agent(
        &self,
        Parameters(params): Parameters<RegisterAgentParams>,
    ) -> String {
        let namespace = match parse_namespace(&params.namespace) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let cmd = RegisterAgent {
            namespace: namespace.clone(),
            roles: params.roles,
            description: params.description,
            metadata: HashMap::new(),
        };

        match self.container.agent_service.register(cmd).await {
            Ok(agent) => {
                self.set_session(agent.id, namespace);
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
        let (agent_id, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return e,
        };

        match self.container.agent_service.heartbeat(&agent_id).await {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Create a new task. Namespace defaults to session namespace; \
        if provided, the project prefix must match.")]
    async fn post_task(&self, Parameters(params): Parameters<PostTaskParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
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

    #[tool(description = "Get the next available task for the session agent, optionally filtered \
        by namespace (defaults to session namespace) and role.")]
    async fn get_next_task(&self, Parameters(params): Parameters<GetNextTaskParams>) -> String {
        let (agent_id, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let roles = match params.role {
            Some(r) => vec![r],
            None => {
                match self.container.agent_service.get(&agent_id).await {
                    Ok(agent) => agent.roles,
                    Err(e) => return format!("error fetching agent roles: {e}"),
                }
            }
        };

        match self
            .container
            .task_service
            .get_next(&agent_id, &roles, Some(namespace))
            .await
        {
            Ok(Some(task)) => to_json(&task),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List tasks, optionally filtered by namespace (defaults to session \
        namespace) and status.")]
    async fn list_tasks(&self, Parameters(params): Parameters<ListTasksParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
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

        if params.status.is_some() && status == Some(None) {
            return "invalid status value".to_string();
        }

        let filter = TaskFilter {
            namespace: Some(namespace),
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
        let (agent_id, _) = match self.require_session() {
            Ok(s) => s,
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

    #[tool(description = "Write a key-value entry to shared memory. Namespace defaults to \
        session namespace; if provided, the project prefix must match.")]
    async fn write_memory(&self, Parameters(params): Parameters<WriteMemoryParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
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

    #[tool(description = "Read a memory entry by key. Namespace defaults to session namespace.")]
    async fn read_memory(&self, Parameters(params): Parameters<ReadMemoryParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self.container.memory_service.read(&namespace, &params.key).await {
            Ok(Some(entry)) => to_json(&entry),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List memory entries. Namespace defaults to session namespace.")]
    async fn list_memory(&self, Parameters(params): Parameters<ListMemoryParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let filter = MemoryFilter { namespace: Some(namespace) };

        match self.container.memory_service.list(filter).await {
            Ok(entries) => to_json(&entries),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Search memory entries by semantic similarity. Namespace defaults to \
        session namespace.")]
    async fn search_memory(&self, Parameters(params): Parameters<SearchMemoryParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let limit = params.limit.unwrap_or(10) as usize;

        match self
            .container
            .memory_service
            .search(&params.query, Some(&namespace), limit)
            .await
        {
            Ok(entries) => to_json(&entries),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Delete a memory entry by key. Namespace defaults to session namespace.")]
    async fn delete_memory(&self, Parameters(params): Parameters<DeleteMemoryParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self.container.memory_service.delete(&namespace, &params.key).await {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Send a message to another agent (by ID), a role (role:name), or \
        broadcast. Namespace defaults to session namespace.")]
    async fn send_message(&self, Parameters(params): Parameters<SendMessageParams>) -> String {
        let (agent_id, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let target = match MessageTarget::parse(&params.to) {
            Ok(t) => t,
            Err(e) => return format!("invalid target: {e}"),
        };

        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
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

    #[tool(description = "Check the mailbox for pending messages. Namespace defaults to session \
        namespace.")]
    async fn check_mailbox(&self, Parameters(params): Parameters<CheckMailboxParams>) -> String {
        let (agent_id, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self
            .container
            .message_service
            .check(&agent_id, &namespace)
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

    #[tool(description = "Save a context snapshot for the session agent. Namespace defaults to \
        session namespace.")]
    async fn save_context(&self, Parameters(params): Parameters<SaveContextParams>) -> String {
        let (agent_id, _) = match self.require_session() {
            Ok(s) => s,
            Err(e) => return e,
        };

        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
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

    #[tool(description = "Load the most recent context snapshot for an agent (defaults to \
        session agent).")]
    async fn load_context(&self, Parameters(params): Parameters<LoadContextParams>) -> String {
        let agent_id = match params.agent_id.as_deref() {
            Some(id_str) => match parse_agent_id(id_str) {
                Ok(id) => id,
                Err(e) => return e,
            },
            None => match self.require_session() {
                Ok((id, _)) => id,
                Err(e) => return e,
            },
        };

        match self.container.context_service.load(&agent_id).await {
            Ok(Some(snapshot)) => to_json(&snapshot),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List context snapshots. Namespace defaults to session namespace.")]
    async fn list_contexts(&self, Parameters(params): Parameters<ListContextsParams>) -> String {
        let agent_id = match params.agent_id.as_deref().map(parse_agent_id) {
            Some(Ok(id)) => Some(id),
            Some(Err(e)) => return e,
            None => None,
        };

        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self
            .container
            .context_service
            .list(agent_id.as_ref(), &namespace)
            .await
        {
            Ok(snapshots) => to_json(&snapshots),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Search context snapshots by semantic similarity. Namespace defaults \
        to session namespace.")]
    async fn search_contexts(
        &self,
        Parameters(params): Parameters<SearchContextsParams>,
    ) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
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
            .search(&params.query, &namespace, agent_id.as_ref(), limit)
            .await
        {
            Ok(snapshots) => to_json(&snapshots),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Write a project skill — shared instructions/conventions that all \
        agents in this project will receive. Skills are identified by namespace + name. \
        Writing to an existing name updates it. Namespace defaults to session namespace.")]
    async fn write_skill(&self, Parameters(params): Parameters<WriteSkillParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let cmd = WriteSkill {
            namespace,
            name: params.name,
            description: params.description,
            content: params.content,
            written_by: self.get_session_agent(),
        };

        match self.container.skill_service.write(cmd).await {
            Ok(skill) => to_json(&skill),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Read a specific skill by name. Namespace defaults to session namespace.")]
    async fn read_skill(&self, Parameters(params): Parameters<ReadSkillParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self.container.skill_service.read(&namespace, &params.name).await {
            Ok(Some(skill)) => to_json(&skill),
            Ok(None) => "null".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "List skills for the project. If inherited=true, includes skills \
        from parent namespaces with more specific ones taking precedence. Namespace defaults \
        to session namespace.")]
    async fn list_skills(&self, Parameters(params): Parameters<ListSkillsParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let result = if params.inherited.unwrap_or(false) {
            self.container.skill_service.list_with_inherited(&namespace).await
        } else {
            self.container
                .skill_service
                .list(SkillFilter { namespace: Some(namespace) })
                .await
        };

        match result {
            Ok(skills) => to_json(&skills),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Delete a skill by name. Namespace defaults to session namespace.")]
    async fn delete_skill(&self, Parameters(params): Parameters<DeleteSkillParams>) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        match self.container.skill_service.delete(&namespace, &params.name).await {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("error: {e}"),
        }
    }

    #[tool(description = "Generate a full bootstrap prompt for this project. Contains all \
        orchy instructions, coordination patterns, and project skills in a single text block. \
        Useful for agents that don't support MCP server instructions natively — copy-paste \
        this into their system prompt. Also available as HTTP GET /bootstrap/<namespace>.")]
    async fn get_bootstrap_prompt(
        &self,
        Parameters(params): Parameters<GetBootstrapPromptParams>,
    ) -> String {
        let namespace = match self.resolve_namespace(params.namespace.as_deref()) {
            Ok(ns) => ns,
            Err(e) => return e,
        };

        let host = &self.container.config.server.host;
        let port = self.container.config.server.port;

        match crate::bootstrap::generate_bootstrap_prompt(
            &namespace,
            host,
            port,
            &self.container.skill_service,
        )
        .await
        {
            Ok(prompt) => prompt,
            Err(e) => format!("error: {e}"),
        }
    }
}

const INSTRUCTIONS: &str = "\
orchy — multi-agent coordination server.

You are part of a coordinated multi-agent system. orchy provides shared \
infrastructure: a task board, shared memory, messaging, and skills. You bring \
the intelligence; orchy enforces the rules.

## On Session Start

1. Call `register_agent` with your project namespace, roles, and description.
2. Call `list_skills(inherited=true)` and follow the project conventions.
3. Call `get_next_task` to claim work, or `check_mailbox` for messages.
4. Call `heartbeat` every ~30s to signal liveness.

## Namespace Rules

Every session is scoped to a project namespace (first path segment). \
Sub-scopes are allowed (e.g. `my-project/backend`), but the project prefix \
must always match. You cannot access other projects.

## Coordination

- Claim tasks before working. Complete them with a summary when done.
- Use shared memory to store decisions and context for other agents.
- Use messages to coordinate with teammates.
- Save context before your session ends for continuity.
- Use `list_skills(inherited=true)` to get project conventions.

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
        let namespace = match self.get_session_namespace() {
            Some(ns) => ns,
            None => {
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
            .list_with_inherited(&namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let prompts = skills
            .into_iter()
            .map(|s| Prompt::new(s.name, Some(s.description), None))
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
        let namespace = self
            .get_session_namespace()
            .ok_or_else(|| ErrorData::internal_error("no session namespace", None))?;

        let skill = self
            .container
            .skill_service
            .read(&namespace, &request.name)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let skill = match skill {
            Some(s) => s,
            None => {
                let inherited = self
                    .container
                    .skill_service
                    .list_with_inherited(&namespace)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                inherited
                    .into_iter()
                    .find(|s| s.name == request.name)
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
            skill.content,
        )]);
        result.description = Some(skill.description);
        Ok(result)
    }
}
