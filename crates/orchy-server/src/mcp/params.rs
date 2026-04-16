use rmcp::schemars;
use serde::Deserialize;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentParams {
    pub project: String,
    pub namespace: Option<String>,
    /// Auto-assigned from task demand if omitted.
    pub roles: Option<Vec<String>>,
    #[serde(default)]
    pub description: String,
    /// Short human-readable name for the agent (e.g. "backend-coder").
    pub alias: Option<String>,
    /// Resume this orchy agent after a new MCP session (e.g. orchy or client restarted).
    /// Use the `id` from your last successful `register_agent` response or handoff knowledge.
    pub agent_id: Option<String>,
    /// Create as a child of this parent agent.
    pub parent_id: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ChangeRolesParams {
    pub roles: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListAgentsParams {
    /// Required if not registered yet.
    pub project: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetAliasParams {
    /// Alias to set. Omit or null to clear.
    pub alias: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveAgentParams {
    pub namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PostTaskParams {
    pub namespace: Option<String>,
    /// Parent task ID to create a subtask.
    pub parent_id: Option<String>,
    pub title: String,
    pub description: String,
    /// low, normal (default), high, critical.
    pub priority: Option<String>,
    /// Roles that can claim this task. Empty = any role.
    pub assigned_roles: Option<Vec<String>>,
    /// Task IDs that must complete before this task can be claimed.
    pub depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetNextTaskParams {
    pub namespace: Option<String>,
    /// Defaults to all agent roles.
    pub role: Option<String>,
    /// When true (default), claims the task. When false, returns the top candidate without claiming.
    pub claim: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListTasksParams {
    pub namespace: Option<String>,
    /// pending, blocked, claimed, in_progress, completed, failed, cancelled.
    pub status: Option<String>,
    /// Filter by parent task ID to list subtasks.
    pub parent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ClaimTaskParams {
    pub task_id: String,
    /// When true, moves claimed → in_progress in the same call.
    pub start: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CompleteTaskParams {
    pub task_id: String,
    /// Visible to other agents and parent tasks.
    pub summary: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct StartTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FailTaskParams {
    pub task_id: String,
    pub reason: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CancelTaskParams {
    pub task_id: String,
    pub reason: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskParams {
    pub task_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// low, normal, high, critical.
    pub priority: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UnblockTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AssignTaskParams {
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddTaskNoteParams {
    pub task_id: String,
    pub body: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SplitTaskParams {
    pub task_id: String,
    pub subtasks: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SubtaskParam {
    pub title: String,
    pub description: String,
    /// low, normal (default), high, critical.
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReplaceTaskParams {
    pub task_id: String,
    pub reason: Option<String>,
    pub replacements: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddDependencyParams {
    pub task_id: String,
    pub dependency_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RemoveDependencyParams {
    pub task_id: String,
    pub dependency_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MergeTasksParams {
    /// At least 2 task UUIDs. Must be pending, blocked, or claimed.
    pub task_ids: Vec<String>,
    pub title: String,
    pub description: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DelegateTaskParams {
    /// Parent task to delegate from (stays claimed).
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveTaskParams {
    pub task_id: String,
    pub new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SendMessageParams {
    /// Agent UUID, "role:name", or "broadcast".
    pub to: String,
    pub body: String,
    pub namespace: Option<String>,
    /// Creates a thread.
    pub reply_to: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckMailboxParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckSentMessagesParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MarkReadParams {
    pub message_ids: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListConversationParams {
    pub message_id: String,
    /// Most recent N messages.
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetProjectOverviewParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetProjectParams {
    /// When true, adds summary: agent count, tasks by status, recent completions.
    pub include_summary: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateProjectParams {
    pub description: Option<String>,
    /// Expected version for optimistic concurrency.
    pub version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetProjectMetadataParams {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListNamespacesParams {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TagTaskParams {
    pub task_id: String,
    pub tag: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UntagTaskParams {
    pub task_id: String,
    pub tag: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LockResourceParams {
    pub name: String,
    pub namespace: Option<String>,
    /// Seconds until auto-expiry. Default 300.
    pub ttl_secs: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UnlockResourceParams {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckLockParams {
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReleaseTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListTagsParams {
    pub namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct WatchTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UnwatchTaskParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RequestReviewParams {
    pub task_id: String,
    pub reviewer_agent: Option<String>,
    /// Target reviewer role (e.g. "reviewer").
    pub reviewer_role: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ResolveReviewParams {
    pub review_id: String,
    pub approved: bool,
    pub comments: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListReviewsParams {
    pub task_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetReviewParams {
    pub review_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PollUpdatesParams {
    /// ISO 8601 timestamp. Returns events after this time.
    pub since: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct WriteKnowledgeParams {
    pub namespace: Option<String>,
    /// Hierarchical path (e.g. "db-choice" or "auth/jwt-strategy").
    pub path: String,
    /// Kind string; use list_knowledge_types for the full set (includes skill).
    pub kind: String,
    pub title: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    /// Expected version for optimistic concurrency.
    pub version: Option<u64>,
    /// JSON object of string key-value pairs merged into entry metadata (updates only).
    pub metadata: Option<String>,
    /// Metadata keys to remove before applying `metadata` (updates only; ignored on create).
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PatchKnowledgeMetadataParams {
    pub namespace: Option<String>,
    pub path: String,
    /// JSON object merged into metadata (set or overwrite keys).
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
    pub version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ReadKnowledgeParams {
    pub namespace: Option<String>,
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListKnowledgeParams {
    pub namespace: Option<String>,
    /// Filter by kind; see list_knowledge_types (includes skill).
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchKnowledgeParams {
    pub query: String,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DeleteKnowledgeParams {
    pub namespace: Option<String>,
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AppendKnowledgeParams {
    pub namespace: Option<String>,
    pub path: String,
    pub kind: String,
    pub value: String,
    /// Defaults to "\n".
    pub separator: Option<String>,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub new_namespace: String,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RenameKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub new_path: String,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ChangeKnowledgeKindParams {
    pub path: String,
    pub namespace: Option<String>,
    /// Target kind; use list_knowledge_types for valid values.
    pub kind: String,
    /// Expected version for optimistic concurrency (after change, version increments).
    pub version: Option<u64>,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TagKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub tag: String,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UntagKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub tag: String,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListKnowledgeTypesParams {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ImportKnowledgeParams {
    pub source_project: String,
    pub path: String,
    pub source_namespace: Option<String>,
    pub metadata: Option<String>,
    pub metadata_remove: Option<Vec<String>>,
}
