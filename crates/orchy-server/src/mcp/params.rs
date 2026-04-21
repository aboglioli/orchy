use rmcp::schemars;
use serde::Deserialize;

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RegisterAgentParams {
    pub alias: String,
    pub project: String,
    pub namespace: Option<String>,
    pub organization: Option<String>,
    pub description: String,
    /// Auto-assigned from task demand if omitted.
    pub roles: Option<Vec<String>>,
    /// Informative only (e.g. claude-code, opencode, pi). Not part of identity.
    pub agent_type: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ChangeRolesParams {
    pub roles: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RenameAliasParams {
    pub new_alias: String,
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
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    /// low, normal (default), high, critical.
    pub priority: Option<String>,
    /// Roles that can claim this task. Empty = any role.
    pub assigned_roles: Option<Vec<String>>,
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
    pub links: Option<Vec<LinkParamInput>>,
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
    pub acceptance_criteria: Option<String>,
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
pub struct SplitTaskParams {
    pub task_id: String,
    pub subtasks: Vec<SubtaskParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SubtaskParam {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
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
    pub acceptance_criteria: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DelegateTaskParams {
    /// Parent task to delegate from (stays claimed).
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MoveTaskParams {
    pub task_id: String,
    pub new_namespace: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RefParam {
    /// Resource kind: "task", "knowledge", "agent", "message"
    pub kind: String,
    /// UUID for task/agent/message; path for knowledge (e.g. "auth/jwt-decision")
    pub id: String,
    /// Optional human-readable label shown to recipient
    pub display: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SendMessageParams {
    /// Agent UUID, "role:name", or "broadcast".
    pub to: String,
    pub body: String,
    pub namespace: Option<String>,
    /// Creates a thread.
    pub reply_to: Option<String>,
    /// Context pointers for the recipient. Not graph edges — annotations saying "look at these."
    /// Example: [{"kind":"task","id":"<uuid>","display":"fix auth bug"},{"kind":"knowledge","id":"auth/jwt-strategy"}]
    pub refs: Option<Vec<RefParam>>,
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
pub struct GetAgentContextParams {
    pub relations: Option<RelationOptionsParam>,
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
    pub include_dependencies: Option<bool>,
    pub include_knowledge: Option<bool>,
    pub knowledge_limit: Option<u32>,
    pub knowledge_kind: Option<String>,
    pub knowledge_tag: Option<String>,
    pub knowledge_content_limit: Option<usize>,
    pub relations: Option<RelationOptionsParam>,
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
    /// Optional task ID. When provided, auto-creates a Task→Knowledge Produces edge.
    pub task_id: Option<String>,
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
    pub relations: Option<RelationOptionsParam>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct ListKnowledgeParams {
    pub namespace: Option<String>,
    /// Filter by kind; see list_knowledge_types (includes skill).
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    pub project: Option<String>,
    /// Cursor for pagination (entry ID from next_cursor of previous page).
    pub after: Option<String>,
    /// Max items per page.
    pub limit: Option<u32>,
    /// When true: only entries with no incoming produces/owned_by edges (unlinked knowledge).
    /// When false: only entries with at least one such link.
    /// Omit to return all entries regardless of link status.
    pub orphaned: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchKnowledgeParams {
    pub query: String,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
    pub project: Option<String>,
    /// Minimum similarity score (0.0–1.0). Only applies when embeddings are configured.
    /// Results without a score are always included.
    pub min_score: Option<f32>,
    /// Resource kind for anchor proximity boost (e.g. "task", "agent").
    pub anchor_kind: Option<String>,
    /// Resource ID for anchor proximity boost. Entries linked to this resource score +0.2.
    pub anchor_id: Option<String>,
    /// Task ID for task-subgraph proximity boost. Entries linked to the task's dependency graph (BFS depth 3) score +0.2.
    pub task_id: Option<String>,
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
    /// Relationship type: derived_from, produces, supersedes, merged_from, summarizes, implements, spawns, related_to, depends_on.
    pub rel_type: String,
    /// When true (default), skip creating if an identical edge already exists.
    pub if_not_exists: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RemoveEdgeParams {
    pub edge_id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AssembleContextParams {
    /// Resource kind: "task", "knowledge", "agent"
    pub kind: String,
    /// Resource ID
    pub id: String,
    /// Character budget for all content blocks combined. Default: 4000.
    pub max_tokens: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RelationOptionsParam {
    /// Relation type filter. Omit = kind-aware defaults. Empty array = all types.
    /// Values: derived_from, produces, implements, spawns, summarizes, merged_from,
    ///         depends_on, supersedes, supported_by, contradicted_by, invalidates,
    ///         owned_by, reviewed_by, related_to.
    /// Aliases: blocks/requires/needs → depends_on, creates/made/wrote → produces,
    ///          fulfills/executes → implements, child_of/parent_of → spawns, based_on/from → derived_from.
    pub rel_types: Option<Vec<String>>,
    /// Entity kinds to include. Empty = all. Values: task, knowledge, agent, message.
    pub target_kinds: Option<Vec<String>>,
    /// outgoing, incoming, or both (default).
    pub direction: Option<String>,
    /// Max hops from anchor (default 1, max recommended 5).
    pub max_depth: Option<u32>,
    /// Max relations returned (default 50).
    pub limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct QueryRelationsParams {
    /// Resource kind: task, knowledge, agent, message.
    pub anchor_kind: String,
    /// UUID for task/agent/message; path for knowledge (e.g. "auth/jwt-strategy").
    pub anchor_id: String,
    /// Relation types to filter. Empty or omit = all. See aliases above.
    pub rel_types: Option<Vec<String>>,
    /// Entity kinds to include. Empty = all.
    pub target_kinds: Option<Vec<String>>,
    /// outgoing, incoming, or both (default).
    pub direction: Option<String>,
    /// Point-in-time snapshot: RFC3339 timestamp.
    pub as_of: Option<String>,
    /// Max hops (default 1).
    pub max_depth: Option<u32>,
    /// Max relations returned (default 50).
    pub limit: Option<u32>,
    /// Re-rank Knowledge peers by semantic similarity to this query (requires embeddings).
    pub semantic_query: Option<String>,
    pub namespace: Option<String>,
    pub project: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LinkParamInput {
    /// Resource kind: task, knowledge, agent, message.
    pub to_kind: String,
    /// Path for knowledge; UUID for other kinds.
    pub to_id: String,
    /// Relation type. Supports aliases: "blocks"/"requires"/"needs" → depends_on,
    /// "creates"/"made"/"wrote" → produces, "fulfills"/"executes" → implements,
    /// "child_of"/"parent_of" → spawns, "based_on"/"from" → derived_from.
    pub rel_type: String,
}
