use rmcp::schemars;
use serde::Deserialize;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentParams {
    pub project: String,
    pub namespace: Option<String>,
    pub organization: Option<String>,
    /// Auto-assigned from task demand if omitted.
    pub roles: Option<Vec<String>>,
    pub description: Option<String>,
    /// Human-readable agent id. Stable reconnection key: if an agent with this id already exists
    /// in the project, the session resumes it. Auto-generated if omitted.
    pub id: Option<String>,
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
pub struct SwitchContextParams {
    /// Target project. Resets namespace to root unless namespace also provided.
    pub project: Option<String>,
    /// Target namespace within the project.
    pub namespace: Option<String>,
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
    /// Override the session project to query another project.
    pub project: Option<String>,
    /// Cursor for pagination (task ID from next_cursor of previous page).
    pub after: Option<String>,
    /// Max items per page.
    pub limit: Option<u32>,
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
    pub agent: String,
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
    pub project: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CheckSentMessagesParams {
    pub namespace: Option<String>,
    pub project: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
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
pub struct GetAgentContextParams {}

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
pub struct ListNamespacesParams {
    pub project: Option<String>,
}

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
pub struct PollUpdatesParams {
    /// ISO 8601 timestamp. Returns events after this time.
    pub since: Option<String>,
    pub limit: Option<u32>,
    pub project: Option<String>,
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
    pub project: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListKnowledgeParams {
    pub namespace: Option<String>,
    /// Filter by kind; see list_knowledge_types (includes skill).
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    pub agent: Option<String>,
    pub project: Option<String>,
    /// Cursor for pagination (entry ID from next_cursor of previous page).
    pub after: Option<String>,
    /// Max items per page.
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchKnowledgeParams {
    pub query: String,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
    pub project: Option<String>,
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
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RenameKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub new_path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ChangeKnowledgeKindParams {
    pub path: String,
    pub namespace: Option<String>,
    /// Target kind; use list_knowledge_types for valid values.
    pub kind: String,
    /// Expected version for optimistic concurrency (after change, version increments).
    pub version: Option<u64>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TagKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub tag: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UntagKnowledgeParams {
    pub path: String,
    pub namespace: Option<String>,
    pub tag: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListKnowledgeTypesParams {}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ImportKnowledgeParams {
    pub source_project: String,
    pub path: String,
    pub source_namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddEdgeParams {
    /// Source resource kind: task, knowledge, agent. (message not allowed)
    pub from_kind: String,
    pub from_id: String,
    /// Target resource kind: task, knowledge, agent. (message not allowed)
    pub to_kind: String,
    pub to_id: String,
    /// Relationship type: derived_from, produces, supersedes, merged_from, summarizes, implements, spawns, related_to.
    pub rel_type: String,
    /// Optional human-readable label for the edge.
    pub display: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RemoveEdgeParams {
    pub edge_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetNeighborsParams {
    /// Resource kind: task, knowledge, agent.
    pub kind: String,
    pub id: String,
    /// outgoing, incoming, or omit for both (default).
    pub direction: Option<String>,
    /// Filter by relationship type.
    pub rel_type: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct GetGraphParams {
    /// Resource kind: task, knowledge, agent.
    pub kind: String,
    pub id: String,
    /// Max traversal depth (default 3, max 10).
    pub max_depth: Option<u32>,
    /// Filter to specific relationship types.
    pub rel_types: Option<Vec<String>>,
    /// outgoing (default), incoming, or both.
    pub direction: Option<String>,
    /// When true, include a `nodes` map with title/label for each touched resource.
    pub include_nodes: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListEdgesParams {
    /// Filter by relationship type. Omit to list all edge types.
    pub rel_type: Option<String>,
    /// Cursor from previous page's next_cursor field.
    pub after: Option<String>,
    pub limit: Option<u32>,
}
