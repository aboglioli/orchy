use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};

use super::handler::OrchyHandler;
use super::params::*;

mod agent;
mod edge;
mod knowledge;
mod message;
mod project;
mod task;

pub(super) fn knowledge_metadata_from_json_str(
    raw: Option<&str>,
    label: &'static str,
) -> Result<HashMap<String, String>, String> {
    match raw {
        None | Some("") => Ok(HashMap::new()),
        Some(s) => serde_json::from_str(s).map_err(|e| format!("invalid {label} JSON: {e}")),
    }
}

pub(super) fn optional_knowledge_metadata(
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

pub(super) fn parse_as_of(s: Option<String>) -> std::result::Result<Option<DateTime<Utc>>, String> {
    s.map(|raw| {
        DateTime::parse_from_rfc3339(&raw)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| format!("invalid as_of timestamp: {e}"))
    })
    .transpose()
}

pub(super) fn parse_direction(s: Option<&str>) -> orchy_core::graph::TraversalDirection {
    match s {
        Some("outgoing") => orchy_core::graph::TraversalDirection::Outgoing,
        Some("incoming") => orchy_core::graph::TraversalDirection::Incoming,
        _ => orchy_core::graph::TraversalDirection::Both,
    }
}

pub(super) fn parse_rel_type_alias(
    s: &str,
) -> std::result::Result<orchy_core::graph::RelationType, String> {
    let canonical = match s {
        "blocks" | "requires" | "needs" => "depends_on",
        "creates" | "made" | "wrote" => "produces",
        "fulfills" | "executes" => "implements",
        "child_of" | "parent_of" => "spawns",
        "based_on" | "from" => "derived_from",
        other => other,
    };
    canonical
        .parse::<orchy_core::graph::RelationType>()
        .map_err(|e| e.to_string())
}

pub(super) fn parse_relation_options(
    p: Option<super::params::RelationOptionsParam>,
) -> Option<orchy_core::graph::relation_options::RelationOptions> {
    p.map(
        |opts| orchy_core::graph::relation_options::RelationOptions {
            rel_types: opts.rel_types.map(|v| {
                v.into_iter()
                    .filter_map(|s| parse_rel_type_alias(&s).ok())
                    .collect()
            }),
            target_kinds: opts
                .target_kinds
                .unwrap_or_default()
                .into_iter()
                .filter_map(|s| s.parse::<orchy_core::resource_ref::ResourceKind>().ok())
                .collect(),
            direction: parse_direction(opts.direction.as_deref()),
            max_depth: opts.max_depth.unwrap_or(1),
            limit: opts.limit.unwrap_or(50),
        },
    )
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
        agent::register_agent(self, params).await
    }

    #[tool(
        description = "Whether this MCP session is bound to an orchy agent, and how to resume \
        after an orchy or MCP transport restart. Does not require registration. Call after the \
        client has reconnected (new MCP initialize) if tools failed with session errors or you \
        are unsure whether you still need register_agent."
    )]
    async fn session_status(&self) -> Result<String, String> {
        agent::session_status(self).await
    }

    #[tool(
        description = "List agents in a project. Works before registration if project is passed."
    )]
    async fn list_agents(
        &self,
        Parameters(params): Parameters<ListAgentsParams>,
    ) -> Result<String, String> {
        agent::list_agents(self, params).await
    }

    #[tool(
        description = "Change the roles of the session agent. Affects which tasks \
        get_next_task returns."
    )]
    async fn change_roles(
        &self,
        Parameters(params): Parameters<ChangeRolesParams>,
    ) -> Result<String, String> {
        agent::change_roles(self, params).await
    }

    #[tool(description = "Send a heartbeat for the session agent to signal liveness.")]
    async fn heartbeat(&self) -> Result<String, String> {
        agent::heartbeat(self).await
    }

    #[tool(
        description = "Rename your agent's alias. Since all internal references use UUID, nothing breaks. \
        Only affects: future register_agent lookups, message display, config. \
        New alias must be unique per (org, project) and pass format validation."
    )]
    async fn rename_alias(
        &self,
        Parameters(params): Parameters<RenameAliasParams>,
    ) -> Result<String, String> {
        agent::rename_alias(self, params.new_alias).await
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
        agent::switch_context(self, params).await
    }

    #[tool(
        description = "Get everything you need in one call: your agent info, project metadata, \
        inbox messages, pending tasks matching your roles, skills, and handoff context from \
        previous sessions. Call this after register_agent to bootstrap quickly."
    )]
    async fn get_agent_context(
        &self,
        Parameters(params): Parameters<GetAgentContextParams>,
    ) -> Result<String, String> {
        agent::get_agent_context(self, params).await
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
        agent::poll_updates(self, params).await
    }

    #[tool(
        description = "Check your mailbox for incoming messages. Returns unread and recent \
        messages addressed to you."
    )]
    async fn check_mailbox(
        &self,
        Parameters(params): Parameters<CheckMailboxParams>,
    ) -> Result<String, String> {
        agent::check_mailbox(self, params).await
    }

    #[tool(description = "List messages you have sent, with delivery and read status.")]
    async fn check_sent_messages(
        &self,
        Parameters(params): Parameters<CheckSentMessagesParams>,
    ) -> Result<String, String> {
        agent::check_sent_messages(self, params).await
    }

    #[tool(description = "Mark messages as read by their IDs for the session agent.")]
    async fn mark_read(
        &self,
        Parameters(params): Parameters<MarkReadParams>,
    ) -> Result<String, String> {
        agent::mark_read(self, params).await
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
        agent::list_conversation(self, params).await
    }

    #[tool(description = "Create a task. \
        Use acceptance_criteria to define a clear definition of done. \
        Set hierarchy and dependencies afterwards with add_edge \
        (spawns for parent-child, depends_on for dependencies).")]
    async fn post_task(
        &self,
        Parameters(params): Parameters<PostTaskParams>,
    ) -> Result<String, String> {
        task::post_task(self, params).await
    }

    #[tool(description = "Get the next available task matching your roles. \
        When claim is true (default), claims the task and returns full context (ancestors, children). \
        When claim is false, returns the top candidate without claiming (peek). \
        Skips tasks with incomplete dependencies.")]
    async fn get_next_task(
        &self,
        Parameters(params): Parameters<GetNextTaskParams>,
    ) -> Result<String, String> {
        task::get_next_task(self, params).await
    }

    #[tool(
        description = "List tasks, optionally filtered by namespace and status. \
        Use archived=true to list archived tasks or archived=false to limit to active tasks. \
        Defaults to session namespace; pass namespace=/ to see all namespaces."
    )]
    async fn list_tasks(
        &self,
        Parameters(params): Parameters<ListTasksParams>,
    ) -> Result<String, String> {
        task::list_tasks(self, params).await
    }

    #[tool(description = "Claim a specific task for the session agent. \
        When start is true, moves claimed → in_progress in the same call.")]
    async fn claim_task(
        &self,
        Parameters(params): Parameters<ClaimTaskParams>,
    ) -> Result<String, String> {
        task::claim_task(self, params).await
    }

    #[tool(description = "Start a claimed task (claimed → in_progress). \
        Must be claimed by you first. Returns task with full context.")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<StartTaskParams>,
    ) -> Result<String, String> {
        task::start_task(self, params).await
    }

    #[tool(
        description = "Update the last_activity_at timestamp on a task to keep it from going stale. \
        Call this periodically for long-running work."
    )]
    async fn touch_task(
        &self,
        Parameters(params): Parameters<TouchTaskParams>,
    ) -> Result<String, String> {
        task::touch_task(self, params.task_id).await
    }

    #[tool(
        description = "Mark a task as completed. Always include a summary: what was done, \
        what was learned, key decisions. Write important findings to memory/documents too."
    )]
    async fn complete_task(
        &self,
        Parameters(params): Parameters<CompleteTaskParams>,
    ) -> Result<String, String> {
        task::complete_task(self, params).await
    }

    #[tool(description = "Mark a task as failed with an optional reason.")]
    async fn fail_task(
        &self,
        Parameters(params): Parameters<FailTaskParams>,
    ) -> Result<String, String> {
        task::fail_task(self, params).await
    }

    #[tool(
        description = "Cancel a task (pending, claimed, in_progress, or blocked). \
        Dependent tasks that were blocked on it are notified."
    )]
    async fn cancel_task(
        &self,
        Parameters(params): Parameters<CancelTaskParams>,
    ) -> Result<String, String> {
        task::cancel_task(self, params).await
    }

    #[tool(
        description = "Archive a task. Only tasks in terminal states (completed, failed, cancelled) can be archived. Archived tasks are hidden from listings."
    )]
    async fn archive_task(
        &self,
        Parameters(params): Parameters<ArchiveTaskParams>,
    ) -> Result<String, String> {
        task::archive_task(self, params).await
    }

    #[tool(description = "Restore an archived task back to active status.")]
    async fn unarchive_task(
        &self,
        Parameters(params): Parameters<UnarchiveTaskParams>,
    ) -> Result<String, String> {
        task::unarchive_task(self, params).await
    }

    #[tool(description = "Update task title, description, and/or priority. \
        Must be pending, claimed, in_progress, or blocked.")]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> Result<String, String> {
        task::update_task(self, params).await
    }

    #[tool(
        description = "Manually unblock a blocked task (e.g. after resolving an external dependency)."
    )]
    async fn unblock_task(
        &self,
        Parameters(params): Parameters<UnblockTaskParams>,
    ) -> Result<String, String> {
        task::unblock_task(self, params).await
    }

    #[tool(
        description = "Assign a claimed or in-progress task to a different agent. \
        The task must already be claimed."
    )]
    async fn assign_task(
        &self,
        Parameters(params): Parameters<AssignTaskParams>,
    ) -> Result<String, String> {
        task::assign_task(self, params).await
    }

    #[tool(
        description = "Split a task into subtasks. The parent task is blocked and will \
        auto-complete when all subtasks finish. Agents should work on subtasks directly, \
        not the parent. Each subtask can have its own acceptance_criteria, priority, and \
        dependencies. Returns the parent (with updated status) and all created subtasks."
    )]
    async fn split_task(
        &self,
        Parameters(params): Parameters<SplitTaskParams>,
    ) -> Result<String, String> {
        task::split_task(self, params).await
    }

    #[tool(
        description = "Replace a task with new tasks. Cancels the original and creates \
        replacements that inherit the original's parent (if any)."
    )]
    async fn replace_task(
        &self,
        Parameters(params): Parameters<ReplaceTaskParams>,
    ) -> Result<String, String> {
        task::replace_task(self, params).await
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
        task::merge_tasks(self, params).await
    }

    #[tool(
        description = "Create a subtask under a claimed/in-progress task without blocking the parent. \
        Unlike split_task, the parent keeps its status."
    )]
    async fn delegate_task(
        &self,
        Parameters(params): Parameters<DelegateTaskParams>,
    ) -> Result<String, String> {
        task::delegate_task(self, params).await
    }

    #[tool(
        description = "Add a dependency to a task. If the dependency is not yet completed, \
        the task will be blocked."
    )]
    async fn add_dependency(
        &self,
        Parameters(params): Parameters<AddDependencyParams>,
    ) -> Result<String, String> {
        task::add_dependency(self, params).await
    }

    #[tool(
        description = "Remove a dependency from a task. If all remaining dependencies are \
        completed, the task will be unblocked."
    )]
    async fn remove_dependency(
        &self,
        Parameters(params): Parameters<RemoveDependencyParams>,
    ) -> Result<String, String> {
        task::remove_dependency(self, params).await
    }

    #[tool(description = "Move a task to a different namespace within the same project.")]
    async fn move_task(
        &self,
        Parameters(params): Parameters<MoveTaskParams>,
    ) -> Result<String, String> {
        task::move_task(self, params).await
    }

    #[tool(description = "Add a tag to a task.")]
    async fn tag_task(
        &self,
        Parameters(params): Parameters<TagTaskParams>,
    ) -> Result<String, String> {
        task::tag_task(self, params).await
    }

    #[tool(description = "Remove a tag from a task.")]
    async fn untag_task(
        &self,
        Parameters(params): Parameters<UntagTaskParams>,
    ) -> Result<String, String> {
        task::untag_task(self, params).await
    }

    #[tool(description = "Release a claimed or in-progress task back to pending.")]
    async fn release_task(
        &self,
        Parameters(params): Parameters<ReleaseTaskParams>,
    ) -> Result<String, String> {
        task::release_task(self, params).await
    }

    #[tool(
        description = "Get a task by its ID with context (ancestors/children). \
        Use include_dependencies=true to also fetch all blocking dependency tasks. \
        Use include_knowledge=true to fetch linked knowledge entries. \
        Avoids N+1 fetch patterns when loading full task context in one call."
    )]
    async fn get_task(
        &self,
        Parameters(params): Parameters<GetTaskParams>,
    ) -> Result<String, String> {
        task::get_task(self, params).await
    }

    #[tool(description = "List all unique tags used across tasks. \
        Defaults to session namespace; pass namespace=/ to see all namespaces.")]
    async fn list_tags(
        &self,
        Parameters(params): Parameters<ListTagsParams>,
    ) -> Result<String, String> {
        task::list_tags(self, params).await
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
        message::send_message(self, params).await
    }

    #[tool(description = "List available knowledge entry types with descriptions.")]
    async fn list_knowledge_types(
        &self,
        Parameters(params): Parameters<ListKnowledgeTypesParams>,
    ) -> Result<String, String> {
        knowledge::list_knowledge_types(self, params).await
    }

    #[tool(description = "Write a knowledge entry. Creates or updates by path. \
        kind is required — use list_knowledge_types for valid values (includes skill). \
        Optional `metadata` is a JSON object merged on update; `metadata_remove` drops keys first.")]
    async fn write_knowledge(
        &self,
        Parameters(params): Parameters<WriteKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::write_knowledge(self, params).await
    }

    #[tool(
        description = "Merge or remove knowledge entry metadata without changing title, content, or kind. \
        `metadata` is a JSON object of string values; `metadata_remove` lists keys to delete first."
    )]
    async fn patch_knowledge_metadata(
        &self,
        Parameters(params): Parameters<PatchKnowledgeMetadataParams>,
    ) -> Result<String, String> {
        knowledge::patch_knowledge_metadata(self, params).await
    }

    #[tool(description = "Read a knowledge entry by path. \
        Defaults to session namespace; pass namespace=/ for root.")]
    async fn read_knowledge(
        &self,
        Parameters(params): Parameters<ReadKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::read_knowledge(self, params).await
    }

    #[tool(
        description = "List knowledge entries with optional filters: type, tag, \
        path_prefix, namespace, orphaned, and archived. Archived entries are hidden from default listings."
    )]
    async fn list_knowledge(
        &self,
        Parameters(params): Parameters<ListKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::list_knowledge(self, params).await
    }

    #[tool(
        description = "Search knowledge entries by semantic similarity or keyword. \
        Results include score: Option<f32> (0.0–1.0 similarity). \
        Use min_score to filter low-confidence results (e.g. min_score=0.75). \
        Defaults to session namespace; pass namespace=/ to search all namespaces."
    )]
    async fn search_knowledge(
        &self,
        Parameters(params): Parameters<SearchKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::search_knowledge(self, params).await
    }

    #[tool(description = "Delete a knowledge entry by path.")]
    async fn delete_knowledge(
        &self,
        Parameters(params): Parameters<DeleteKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::delete_knowledge(self, params).await
    }

    #[tool(
        description = "Archive a knowledge entry. Archived entries are hidden from listings but remain accessible via edge traversal and explicit retrieval."
    )]
    async fn archive_knowledge(
        &self,
        Parameters(params): Parameters<ArchiveKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::archive_knowledge(self, params).await
    }

    #[tool(description = "Restore an archived knowledge entry back to active status.")]
    async fn unarchive_knowledge(
        &self,
        Parameters(params): Parameters<UnarchiveKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::unarchive_knowledge(self, params).await
    }

    #[tool(description = "Append text to a knowledge entry. Creates if it doesn't exist.")]
    async fn append_knowledge(
        &self,
        Parameters(params): Parameters<AppendKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::append_knowledge(self, params).await
    }

    #[tool(description = "Move a knowledge entry to a different namespace.")]
    async fn move_knowledge(
        &self,
        Parameters(params): Parameters<MoveKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::move_knowledge(self, params).await
    }

    #[tool(description = "Rename a knowledge entry's path.")]
    async fn rename_knowledge(
        &self,
        Parameters(params): Parameters<RenameKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::rename_knowledge(self, params).await
    }

    #[tool(description = "Change the kind of an existing knowledge entry. \
        Does not run on `write_knowledge` updates — use this tool explicitly. \
        Bumps version when the kind actually changes.")]
    async fn change_knowledge_kind(
        &self,
        Parameters(params): Parameters<ChangeKnowledgeKindParams>,
    ) -> Result<String, String> {
        knowledge::change_knowledge_kind(self, params).await
    }

    #[tool(description = "Add a tag to a knowledge entry.")]
    async fn tag_knowledge(
        &self,
        Parameters(params): Parameters<TagKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::tag_knowledge(self, params).await
    }

    #[tool(description = "Remove a tag from a knowledge entry.")]
    async fn untag_knowledge(
        &self,
        Parameters(params): Parameters<UntagKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::untag_knowledge(self, params).await
    }

    #[tool(description = "Import a knowledge entry from a linked project.")]
    async fn import_knowledge(
        &self,
        Parameters(params): Parameters<ImportKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::import_knowledge(self, params).await
    }

    #[tool(
        description = "Assemble structured context for a resource by following its graph edges. \
        Returns: core_facts (Produces/DerivedFrom linked knowledge), open_dependencies (incomplete task deps), \
        relevant_decisions (Decision/Plan/Skill knowledge), recent_changes (latest 5 other linked knowledge), \
        risk_flags (failed/cancelled tasks in dependency path). \
        Use instead of multiple query_relations calls when you need a full context snapshot for a task."
    )]
    async fn assemble_context(
        &self,
        Parameters(params): Parameters<AssembleContextParams>,
    ) -> Result<String, String> {
        knowledge::assemble_context(self, params).await
    }

    #[tool(
        description = "Promote a knowledge entry (decision, discovery, or pattern) to a skill. \
        Creates a new kind=skill entry derived from the source, with an optional instruction prefix. \
        Establishes a DerivedFrom edge from the new skill to the source entry."
    )]
    async fn promote_knowledge(
        &self,
        Parameters(params): Parameters<PromoteKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::promote_knowledge(self, params).await
    }

    #[tool(
        description = "Consolidate multiple knowledge entries into a single new entry. \
        Concatenates source contents, creates a new entry with MergedFrom edges \
        pointing to each source, then deletes the original entries. \
        Requires at least 2 source paths."
    )]
    async fn consolidate_knowledge(
        &self,
        Parameters(params): Parameters<ConsolidateKnowledgeParams>,
    ) -> Result<String, String> {
        knowledge::consolidate_knowledge(self, params).await
    }

    #[tool(
        description = "Get the project metadata for the current session's project. \
        Set include_summary to add agent/task overview (same data as the former get_project_summary)."
    )]
    async fn get_project(
        &self,
        Parameters(params): Parameters<GetProjectParams>,
    ) -> Result<String, String> {
        project::get_project(self, params).await
    }

    #[tool(
        description = "Update the project description and/or metadata for the current session's project."
    )]
    async fn update_project(
        &self,
        Parameters(params): Parameters<UpdateProjectParams>,
    ) -> Result<String, String> {
        project::update_project(self, params).await
    }

    #[tool(description = "Set a metadata key-value pair on the current session's project.")]
    async fn set_project_metadata(
        &self,
        Parameters(params): Parameters<SetProjectMetadataParams>,
    ) -> Result<String, String> {
        project::set_project_metadata(self, params).await
    }

    #[tool(
        description = "List all registered namespaces for the current session's project. \
        Namespaces are auto-registered when agents connect or tasks are created."
    )]
    async fn list_namespaces(
        &self,
        Parameters(params): Parameters<ListNamespacesParams>,
    ) -> Result<String, String> {
        project::list_namespaces(self, params).await
    }

    #[tool(
        description = "Acquire a named distributed lock. Fails if held by another agent. \
        Locks auto-expire after ttl_secs (default 300)."
    )]
    async fn lock_resource(
        &self,
        Parameters(params): Parameters<LockResourceParams>,
    ) -> Result<String, String> {
        project::lock_resource(self, params).await
    }

    #[tool(description = "Release a named distributed lock.")]
    async fn unlock_resource(
        &self,
        Parameters(params): Parameters<UnlockResourceParams>,
    ) -> Result<String, String> {
        project::unlock_resource(self, params).await
    }

    #[tool(description = "Check if a resource lock exists without acquiring it.")]
    async fn check_lock(
        &self,
        Parameters(params): Parameters<CheckLockParams>,
    ) -> Result<String, String> {
        project::check_lock(self, params).await
    }

    #[tool(description = "Create a typed directed edge between two resources. \
        Relationship types: derived_from, produces, supersedes, merged_from, \
        summarizes, implements, spawns, related_to, depends_on. \
        Resource kinds: task, knowledge, agent, message.")]
    async fn add_edge(
        &self,
        Parameters(params): Parameters<AddEdgeParams>,
    ) -> Result<String, String> {
        edge::add_edge(self, params).await
    }

    #[tool(description = "Delete an edge by its ID.")]
    async fn remove_edge(
        &self,
        Parameters(params): Parameters<RemoveEdgeParams>,
    ) -> Result<String, String> {
        edge::remove_edge(self, params).await
    }

    #[tool(
        description = r#"Query graph relations for an entity. Returns EntityNeighborhood with inlined peer data.

Use when you need to explore what an entity is connected to without fetching the full entity.
Use semantic_query to re-rank knowledge peers by embedding similarity (requires embeddings config).

anchor_kind: task | knowledge | agent | message
anchor_id: UUID for task/agent/message; path for knowledge (e.g. "auth/jwt-strategy")
rel_types: empty or omit = all types. Aliases: blocks→depends_on, creates→produces, etc.
direction: outgoing (anchor→peer) | incoming (peer→anchor) | both (default)
max_depth: default 1 (direct neighbors). Max recommended: 5.

Example — find what blocks my task transitively:
{"anchor_kind":"task","anchor_id":"<uuid>","rel_types":["depends_on"],"direction":"outgoing","max_depth":3}
"#
    )]
    async fn query_relations(
        &self,
        Parameters(params): Parameters<QueryRelationsParams>,
    ) -> Result<String, String> {
        edge::query_relations(self, params).await
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
            self.set_mcp_session_id(session_id).await;
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
            self.set_mcp_session_id(session_id).await;
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
            self.set_mcp_session_id(session_id).await;
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
            self.set_mcp_session_id(session_id).await;
        }
        self.touch_heartbeat();
        let (_, org, project, namespace) = match self.require_session().await {
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
            self.set_mcp_session_id(session_id).await;
        }
        self.touch_heartbeat();
        let (_, org, project, namespace) = self
            .require_session()
            .await
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
