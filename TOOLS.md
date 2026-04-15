# orchy MCP Tools Reference

**69** tools (counted from `crates/orchy-server/src/mcp/tools.rs`). This file
is not auto-generated; for an accurate per-tool list and parameters see
**README.md** (`## MCP Tools`) and `crates/orchy-server/src/mcp/params.rs`.

Tools marked **Session** require `register_agent` first (exceptions: same as README).

## Agent

| Tool | Session | Description |
|------|---------|-------------|
| `register_agent` | no | Register as an agent. Required before almost every other tool. |
| `resume_agent` | no | Resume a previous agent session by ID. Restores project, namespace, roles. |
| `list_agents` | if no `project` | List agents in a project. Works before registration if project is passed. |
| `change_roles` | yes | Change the roles of the session agent. |
| `move_agent` | yes | Move the session agent to a new namespace. |
| `heartbeat` | yes | Signal liveness. All tool calls also act as heartbeats. |
| `disconnect` | yes | Disconnect and release all held resources. |

### `register_agent`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | yes | Project identifier |
| `description` | yes | What this agent is |
| `namespace` | no | Scope within project |
| `roles` | no | Capabilities. Auto-assigned from task demand if omitted. |
| `agent_id` | no | Resume a previous agent session |
| `parent_id` | no | Create as child of this parent agent |

### `resume_agent`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `agent_id` | yes | Agent UUID to resume |

### `list_agents`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | no | Required if not registered yet |

### `change_roles`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `roles` | yes | New role list (replaces existing) |

### `move_agent`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | yes | Target namespace |

---

## Tasks

| Tool | Description |
|------|-------------|
| `post_task` | Create a task. Use `parent_id` for subtasks. |
| `get_task` | Get a task by ID with full context (ancestors + children). |
| `get_next_task` | Claim or peek at the next available task matching your roles. |
| `list_tasks` | List tasks filtered by namespace and/or status. |
| `claim_task` | Claim a task. Optional `start: true` to go straight to in_progress. |
| `start_task` | Start a claimed task (claimed → in_progress). |
| `complete_task` | Complete a task with summary. |
| `fail_task` | Mark a task as failed. |
| `cancel_task` | Cancel a task. Notifies dependent tasks. |
| `update_task` | Update title, description, and/or priority. |
| `unblock_task` | Manually unblock a blocked task. |
| `release_task` | Release a claimed task back to pending. |
| `assign_task` | Reassign a claimed/in-progress task to another agent. |
| `delegate_task` | Create a subtask without blocking the parent. |
| `add_task_note` | Add a timestamped note to a task. |
| `split_task` | Split into subtasks. Parent blocks, auto-completes when all finish. |
| `replace_task` | Cancel original, create replacements inheriting parent. |
| `merge_tasks` | Merge multiple tasks into one. Sources are cancelled. |
| `list_subtasks` | List direct children of a task. |
| `add_dependency` | Add dependency. Blocks if dependency not completed. |
| `remove_dependency` | Remove dependency. Unblocks if all remaining deps completed. |
| `mutate_task_tags` | Add or remove a tag. |
| `list_tags` | List all unique tags in the project. |
| `move_task` | Move to a different namespace. |
| `watch_task` | Watch for status changes (notifications via mailbox). |
| `unwatch_task` | Stop watching. |
| `request_review` | Request review by agent or role. |
| `resolve_review` | Approve or reject a review. |
| `list_reviews` | List reviews for a task. |
| `get_review` | Get a single review by ID. |

All task tools require session.

### `post_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `title` | yes | |
| `description` | yes | |
| `namespace` | no | Defaults to session namespace |
| `parent_id` | no | Parent task ID for subtask |
| `priority` | no | low, normal (default), high, critical |
| `assigned_roles` | no | Roles that can claim. Empty = any. |
| `depends_on` | no | Task IDs that must complete first |

### `get_next_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | Filter by namespace |
| `role` | no | Filter by role. Defaults to all agent roles. |
| `claim` | no | true (default) claims; false peeks without claiming |

### `list_tasks`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | Omit for all project tasks |
| `status` | no | pending, blocked, claimed, in_progress, completed, failed, cancelled |

### `claim_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `start` | no | true to also start (claimed → in_progress) |

### `complete_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `summary` | no | What was done, what was learned |

### `fail_task` / `cancel_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `reason` | no | |

### `update_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `title` | no | |
| `description` | no | |
| `priority` | no | low, normal, high, critical |

### `assign_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `agent_id` | yes | Target agent UUID |

### `delegate_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Parent task (stays claimed) |
| `title` | yes | |
| `description` | yes | |
| `priority` | no | |
| `assigned_roles` | no | |

### `split_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Parent task to split |
| `subtasks` | yes | Array of {title, description, priority?, assigned_roles?, depends_on?} |

### `replace_task`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Task to cancel |
| `reason` | no | |
| `replacements` | yes | Array of subtask definitions |

### `merge_tasks`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_ids` | yes | At least 2 task UUIDs |
| `title` | yes | |
| `description` | yes | |

### `add_dependency` / `remove_dependency`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `dependency_id` | yes | |

### `mutate_task_tags`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `tag` | yes | |
| `action` | yes | "add" or "remove" |

### `request_review`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `reviewer_agent` | no | Specific agent UUID |
| `reviewer_role` | no | Target role (e.g. "reviewer") |

### `resolve_review`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `review_id` | yes | |
| `approved` | yes | boolean |
| `comments` | no | |

### Simple ID-only tools
`get_task`, `start_task`, `release_task`, `unblock_task`, `list_subtasks`, `watch_task`, `unwatch_task`, `list_reviews`, `get_review`, `add_task_note` (+ `body`), `move_task` (+ `new_namespace`), `list_tags` (`namespace` optional).

---

## Knowledge

| Tool | Description |
|------|-------------|
| `list_knowledge_types` | List available kinds with descriptions. |
| `write_knowledge` | Create or update entry by path. |
| `read_knowledge` | Read entry by path. |
| `list_knowledge` | List with filters: kind, tag, path_prefix, agent_id, namespace. |
| `search_knowledge` | Semantic similarity search. |
| `delete_knowledge` | Delete by path. |
| `append_knowledge` | Append text to entry. Creates if missing. |
| `relocate_knowledge` | Move to new namespace or rename path. |
| `mutate_knowledge_tags` | Add or remove a tag. |
| `import_knowledge` | Copy entry from a linked project. |

All knowledge tools require session.

### `write_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | Hierarchical path (e.g. "db-choice", "auth/jwt-strategy") |
| `kind` | yes | note, decision, discovery, pattern, context, document, config, reference, plan, log, skill |
| `title` | yes | |
| `content` | yes | |
| `namespace` | no | Defaults to session namespace |
| `tags` | no | Array of strings |
| `version` | no | Expected version for optimistic concurrency |
| `metadata` | no | JSON string of key-value pairs |

### `read_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `namespace` | no | Defaults to root |

### `list_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | Omit for all |
| `kind` | no | Filter by kind |
| `tag` | no | Filter by tag |
| `path_prefix` | no | Filter by path prefix |
| `agent_id` | no | Filter by author |

### `search_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `query` | yes | Search text |
| `namespace` | no | |
| `kind` | no | Filter results by kind |
| `limit` | no | Max results (default 10) |

### `append_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `kind` | yes | |
| `value` | yes | Text to append |
| `namespace` | no | |
| `separator` | no | Defaults to "\n" |

### `relocate_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | Current path |
| `namespace` | no | Current namespace |
| `new_namespace` | no* | Move to this namespace |
| `new_path` | no* | Rename to this path |

*Exactly one of `new_namespace` or `new_path` required.

### `mutate_knowledge_tags`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `tag` | yes | |
| `action` | yes | "add" or "remove" |
| `namespace` | no | |

### `import_knowledge`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `source_project` | yes | |
| `path` | yes | |
| `source_namespace` | no | |

---

## Messages

| Tool | Session | Description |
|------|---------|-------------|
| `send_message` | yes | Send to agent UUID, "role:name", or "broadcast". |
| `list_messages` | yes | List inbound (default) or outbound messages. |
| `mark_read` | no | Mark messages as read. |
| `list_conversation` | no | Full thread for a message ID. |

### `send_message`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `to` | yes | Agent UUID, "role:name", or "broadcast" |
| `body` | yes | |
| `namespace` | no | |
| `reply_to` | no | Message ID to thread |

### `list_messages`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |
| `direction` | no | "inbound" (default) or "outbound" |

### `mark_read`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `message_ids` | yes | Array of message UUIDs |

### `list_conversation`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `message_id` | yes | Any message in the thread |
| `limit` | no | Most recent N |

---

## Resource Locking

All require session.

### `lock_resource`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `name` | yes | Resource name (e.g. file path) |
| `namespace` | no | |
| `ttl_secs` | no | Auto-expiry seconds (default 300) |

### `unlock_resource`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `name` | yes | |
| `namespace` | no | |

### `check_lock`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `name` | yes | |
| `namespace` | no | |

---

## Project

All require session.

### `get_project`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `include_summary` | no | true to add agent count, task stats, recent completions |

### `update_project`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `description` | yes | |

### `add_project_note`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `body` | yes | |

### `get_agent_workload`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `agent_id` | no | Defaults to current agent |

---

## Project Links

All require session.

### `link_project`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `source_project` | yes | Project to import from |
| `resource_types` | yes | "knowledge", "tasks" |

### `unlink_project`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `source_project` | yes | |

### `list_project_links`
No parameters.

---

## Discovery

All require session.

### `list_namespaces`
No parameters.

### `get_bootstrap_prompt`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |

### `poll_updates`
| Parameter | Required | Description |
|-----------|----------|-------------|
| `since` | no | ISO 8601 timestamp |
| `limit` | no | Max events to return |
