# orchy

Multi-agent coordination server. Shared infrastructure for AI agents: task
board, unified knowledge base, messaging, resource locking, and project
context — exposed as **66** MCP tools over Streamable HTTP.

orchy is not an orchestrator. Agents bring the intelligence; orchy provides
the coordination layer and enforces the rules.

## Quick start

```bash
cargo run -p orchy-server
```

MCP server at `http://127.0.0.1:3100/mcp`. Bootstrap prompt at
`http://127.0.0.1:3100/bootstrap/<project>`.

## Configuration

```toml
[server]
host = "127.0.0.1"
port = 3100
heartbeat_timeout_secs = 300

[store]
backend = "sqlite"              # "sqlite", "postgres", or "memory"

[store.sqlite]
path = "orchy.db"

# [store.postgres]
# url = "postgres://orchy:orchy@localhost:5432/orchy"

# [skills]
# dir = "skills"

# [embeddings]
# provider = "openai"
# [embeddings.openai]
# url = "https://api.openai.com/v1/embeddings"
# model = "text-embedding-3-small"
# dimensions = 1536
```

## Concepts

### Knowledge

All persistent knowledge lives in a unified system with typed entries.
Each entry has a `kind`, `path`, `title`, `content`, `tags`, and `version`.

| Kind | Description |
|------|-------------|
| `note` | General observation or record |
| `decision` | A choice made with rationale |
| `discovery` | Something found or learned |
| `pattern` | A recurring approach or convention |
| `context` | Session summary / agent state snapshot |
| `document` | Long-form structured content |
| `config` | Configuration or setup information |
| `reference` | External reference or link |
| `plan` | Strategy, roadmap, or approach |
| `log` | Activity or change log entry |
| `skill` | Instruction or convention agents must follow |
| `overview` | Project summary included in HTTP/bootstrap prompts |

Paths are hierarchical: `db-choice`, `auth/jwt-strategy`, `api-design`.
Skills (kind=skill) inherit through namespace hierarchy.

### Tasks

```
Pending → Claimed → InProgress → Completed/Failed/Cancelled
```

Tasks support hierarchy (`split_task`), dependencies, tags, watchers,
and reviews. Parent tasks auto-complete when all subtasks finish.

### Agent lifecycle

1. Register with `register_agent` (roles auto-assigned if omitted)
2. `heartbeat` every ~30s; after registration, MCP tool invocations refresh liveness
3. On disconnect: tasks released, locks freed, watchers removed

### Resource locking

TTL-based locking for any named resource. Auto-expires and cleaned up
on agent disconnect.

### Project links

Projects can link to other projects to share knowledge entries.

### Event log

Every state change is recorded as a semantic domain event. Query with
`poll_updates`.

## MCP Tools

Authoritative definitions: `crates/orchy-server/src/mcp/tools.rs` and
`crates/orchy-server/src/mcp/params.rs`. A running server exposes the current
set via MCP `list_tools`.

**Session** — `yes` means call `register_agent` first. **no** — callable
without registration. **partial** — `list_agents` only: pass `project`,
or register to use the session project.

Tools that do not require registration: `register_agent`, `session_status`,
`list_knowledge_types`, `mark_read`, `list_conversation`, and `list_agents`
when `project` is passed.

---

## Agent

| Tool | Session | Description |
|------|---------|-------------|
| `register_agent` | no | Register as an agent. Required before almost every other tool. |
| `session_status` | no | Check whether this MCP session is bound to an orchy agent. |
| `list_agents` | partial | List agents in a project. Works before registration if `project` is passed. |
| `change_roles` | yes | Change the roles of the session agent. |
| `set_alias` | yes | Set or clear the agent's alias (unique within project). |
| `heartbeat` | yes | Send a heartbeat to signal liveness. |
| `disconnect` | yes | Disconnect and release all claimed tasks back to pending. |
| `move_agent` | yes | Move the session agent to a new namespace within the same project. |

### `register_agent`

Register as an agent. Required before almost every other tool. Roles are
optional — orchy assigns them from pending task demand if omitted. Pass `agent_id`
to resume the same orchy agent after a new MCP session (orchy or client restarted);
persist that UUID from the last `register_agent` JSON or handoff knowledge.
Use `parent_id` for agent lineage.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | yes | Project identifier |
| `description` | yes | What this agent is |
| `namespace` | no | Scope within project |
| `roles` | no | Capabilities. Auto-assigned from task demand if omitted |
| `alias` | no | Short human-readable name (e.g. "backend-coder") |
| `agent_id` | no | Resume a previous agent session by UUID |
| `parent_id` | no | Create as child of this parent agent |
| `metadata` | no | Key-value pairs attached to the agent record |

### `session_status`

Check whether this MCP session is bound to an orchy agent, and how to resume
after an orchy or MCP transport restart. Does not require registration. Call
after the client has reconnected (new MCP initialize) if tools failed with
session errors or you are unsure whether you still need `register_agent`.

### `list_agents`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | no | Required if not registered yet |

### `change_roles`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `roles` | yes | New role list (replaces existing) |

### `set_alias`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `alias` | no | Alias to set. Omit or null to clear. |

### `move_agent`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | yes | Target namespace |

---

## Tasks — Create

| Tool | Session | Description |
|------|---------|-------------|
| `post_task` | yes | Create a task. Use `parent_id` for subtasks. |
| `delegate_task` | yes | Create a subtask without blocking the parent. |
| `split_task` | yes | Split a task into subtasks. Parent blocks, auto-completes when all finish. |
| `replace_task` | yes | Cancel original, create replacements inheriting parent. |
| `merge_tasks` | yes | Merge multiple tasks into one. |

### `post_task`

Create a task. Tasks with `depends_on` are auto-blocked until dependencies complete.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `title` | yes | Task title |
| `description` | yes | Task description |
| `namespace` | no | Defaults to session namespace |
| `parent_id` | no | Parent task ID for subtask |
| `priority` | no | `low`, `normal` (default), `high`, `critical` |
| `assigned_roles` | no | Roles that can claim. Empty = any role |
| `depends_on` | no | Task IDs that must complete before this task can be claimed |

### `delegate_task`

Create a subtask under a claimed/in-progress task without blocking the parent.
Unlike `split_task`, the parent keeps its status.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Parent task to delegate from (stays claimed) |
| `title` | yes | Subtask title |
| `description` | yes | Subtask description |
| `priority` | no | Defaults to parent priority |
| `assigned_roles` | no | Roles that can claim the subtask |

### `split_task`

Split a task into subtasks. The parent task is blocked and will auto-complete
when all subtasks finish. Agents should work on subtasks directly, not the
parent.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Parent task to split |
| `subtasks` | yes | Array of subtask definitions |
| `subtasks[].title` | yes | |
| `subtasks[].description` | yes | |
| `subtasks[].priority` | no | `low`, `normal` (default), `high`, `critical` |
| `subtasks[].assigned_roles` | no | |
| `subtasks[].depends_on` | no | |

### `replace_task`

Cancel a task and create replacements that inherit the original's parent (if any).

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Task to cancel |
| `reason` | no | Reason for replacement |
| `replacements` | yes | Array of subtask definitions (same structure as `split_task`) |

### `merge_tasks`

Merge multiple tasks into one. Source tasks must be pending, blocked, or claimed.
They are cancelled and a new consolidated task is created with the highest
priority, combined roles, combined dependencies, and collected notes.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_ids` | yes | At least 2 task UUIDs. Must be pending, blocked, or claimed |
| `title` | yes | Title for the merged task |
| `description` | yes | Description for the merged task |

---

## Tasks — Lifecycle

| Tool | Session | Description |
|------|---------|-------------|
| `get_next_task` | yes | Claim or peek the next available task matching your roles. |
| `get_task` | yes | Get a task by ID with full context (ancestors + children). |
| `list_tasks` | yes | List tasks filtered by namespace, status, and/or parent. |
| `claim_task` | yes | Claim a specific task for the session agent. |
| `start_task` | yes | Start a claimed task (claimed → in_progress). |
| `complete_task` | yes | Complete a task with a summary. |
| `fail_task` | yes | Mark a task as failed. |
| `cancel_task` | yes | Cancel a task. Notifies dependent tasks. |
| `update_task` | yes | Update title, description, and/or priority. |
| `release_task` | yes | Release a claimed or in-progress task back to pending. |
| `assign_task` | yes | Reassign a claimed/in-progress task to another agent. |
| `unblock_task` | yes | Manually unblock a blocked task. |

### `get_next_task`

Get the next available task matching your roles. When `claim` is true (default),
claims the task and returns full context. When false, returns the top candidate
without claiming (peek). Skips tasks with incomplete dependencies.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | Filter by namespace |
| `role` | no | Filter by role. Defaults to all agent roles |
| `claim` | no | `true` (default) claims; `false` peeks without claiming |

### `list_tasks`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | Omit for all project tasks |
| `status` | no | `pending`, `blocked`, `claimed`, `in_progress`, `completed`, `failed`, `cancelled` |
| `parent_id` | no | Filter by parent task ID to list subtasks |

### `claim_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `start` | no | `true` to also start (claimed → in_progress) in the same call |

### `complete_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `summary` | no | What was done, what was learned. Visible to other agents and parent tasks |

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
| `priority` | no | `low`, `normal`, `high`, `critical` |

### `assign_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `agent_id` | yes | Target agent UUID |

---

## Tasks — Dependencies

| Tool | Session | Description |
|------|---------|-------------|
| `add_dependency` | yes | Add a dependency. Task blocks until dependency completes. |
| `remove_dependency` | yes | Remove a dependency. Unblocks if all remaining deps complete. |

### `add_dependency` / `remove_dependency`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `dependency_id` | yes | |

---

## Tasks — Notes, Tags, and Namespace

| Tool | Session | Description |
|------|---------|-------------|
| `add_task_note` | yes | Add a timestamped note to a task. |
| `tag_task` | yes | Add a tag to a task. |
| `untag_task` | yes | Remove a tag from a task. |
| `list_tags` | yes | List all unique tags used across tasks in the project. |
| `move_task` | yes | Move a task to a different namespace within the same project. |

### `add_task_note`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `body` | yes | Note content |

### `tag_task` / `untag_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `tag` | yes | |

### `move_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `new_namespace` | yes | |

---

## Tasks — Watchers and Reviews

| Tool | Session | Description |
|------|---------|-------------|
| `watch_task` | yes | Watch a task for status changes (notifications via mailbox). |
| `unwatch_task` | yes | Stop watching a task. |
| `request_review` | yes | Request a review for a task (by agent ID or role). |
| `resolve_review` | yes | Approve or reject a review request. |
| `list_reviews` | yes | List review requests for a task. |
| `get_review` | yes | Get a single review request by ID. |

### `watch_task` / `unwatch_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |

### `request_review`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `reviewer_agent` | no | Specific agent UUID |
| `reviewer_role` | no | Target reviewer role (e.g. "reviewer") |

### `resolve_review`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `review_id` | yes | |
| `approved` | yes | `true` to approve, `false` to reject |
| `comments` | no | |

### `list_reviews` / `get_review`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | (for `list_reviews`) |
| `review_id` | yes | (for `get_review`) |

---

## Messages

| Tool | Session | Description |
|------|---------|-------------|
| `send_message` | yes | Send a message to an agent, role, or broadcast. |
| `check_mailbox` | yes | Check your mailbox for incoming messages. |
| `check_sent_messages` | yes | List messages you have sent, with delivery and read status. |
| `mark_read` | no | Mark messages as read by their IDs. |
| `list_conversation` | no | Get the full conversation thread for a message ID. |

### `send_message`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `to` | yes | Agent UUID, `role:name` (all agents with that role), or `broadcast` |
| `body` | yes | Message body |
| `namespace` | no | |
| `reply_to` | no | Message ID to thread the message |

### `check_mailbox` / `check_sent_messages`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |

### `mark_read`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `message_ids` | yes | Array of message UUIDs |

### `list_conversation`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `message_id` | yes | Any message in the thread |
| `limit` | no | Most recent N messages |

---

## Knowledge

| Tool | Session | Description |
|------|---------|-------------|
| `list_knowledge_types` | no | List available knowledge entry types with descriptions. |
| `write_knowledge` | yes | Create or update a knowledge entry by path. |
| `read_knowledge` | yes | Read a knowledge entry by path. |
| `list_knowledge` | yes | List knowledge entries with optional filters. |
| `search_knowledge` | yes | Search knowledge entries by semantic similarity. |
| `delete_knowledge` | yes | Delete a knowledge entry by path. |
| `append_knowledge` | yes | Append text to a knowledge entry. Creates if it doesn't exist. |
| `patch_knowledge_metadata` | yes | Merge or remove metadata without changing content. |
| `move_knowledge` | yes | Move a knowledge entry to a different namespace. |
| `rename_knowledge` | yes | Rename a knowledge entry's path. |
| `change_knowledge_kind` | yes | Change the kind of an existing entry. |
| `tag_knowledge` | yes | Add a tag to a knowledge entry. |
| `untag_knowledge` | yes | Remove a tag from a knowledge entry. |
| `import_knowledge` | yes | Import a knowledge entry from a linked project. |

### `write_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | Hierarchical path (e.g. `db-choice`, `auth/jwt-strategy`) |
| `kind` | yes | Entry kind (see `list_knowledge_types` for valid values) |
| `title` | yes | Entry title |
| `content` | yes | Entry content |
| `namespace` | no | Defaults to session namespace |
| `tags` | no | Array of strings |
| `version` | no | Expected version for optimistic concurrency |
| `metadata` | no | JSON object of string key-value pairs merged on update |
| `metadata_remove` | no | Metadata keys to remove before applying `metadata` |

### `read_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `namespace` | no | Defaults to root namespace |

### `list_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |
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
| `separator` | no | Defaults to `\n` |
| `metadata` | no | JSON object merged into entry metadata |
| `metadata_remove` | no | Metadata keys to remove first |

### `patch_knowledge_metadata`

Merge or remove knowledge entry metadata without changing title, content, or kind.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `namespace` | no | |
| `metadata` | no | JSON object merged into metadata (set or overwrite keys) |
| `metadata_remove` | no | Metadata keys to delete first |
| `version` | no | Expected version for optimistic concurrency |

### `move_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | Current path |
| `new_namespace` | yes | Target namespace |
| `namespace` | no | Current namespace |
| `metadata` | no | JSON object merged into entry metadata |
| `metadata_remove` | no | Metadata keys to remove first |

### `rename_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | Current path |
| `new_path` | yes | New path |
| `namespace` | no | |
| `metadata` | no | JSON object merged into entry metadata |
| `metadata_remove` | no | Metadata keys to remove first |

### `change_knowledge_kind`

Change the kind of an existing knowledge entry. Does not run on `write_knowledge`
updates — use this tool explicitly. Bumps version when the kind actually changes.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `kind` | yes | Target kind (see `list_knowledge_types`) |
| `namespace` | no | |
| `version` | no | Expected version for optimistic concurrency |
| `metadata` | no | JSON object merged into entry metadata |
| `metadata_remove` | no | Metadata keys to remove first |

### `tag_knowledge` / `untag_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `tag` | yes | |
| `namespace` | no | |
| `metadata` | no | JSON object merged into entry metadata |
| `metadata_remove` | no | Metadata keys to remove first |

### `import_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `source_project` | yes | Project to import from |
| `path` | yes | Entry path in source project |
| `source_namespace` | no | Namespace in source project |
| `metadata` | no | JSON object merged into imported entry metadata |
| `metadata_remove` | no | Metadata keys to remove from imported entry |

---

## Resource Locking

| Tool | Session | Description |
|------|---------|-------------|
| `lock_resource` | yes | Acquire a named distributed lock. |
| `unlock_resource` | yes | Release a named distributed lock. |
| `check_lock` | yes | Check if a resource lock exists without acquiring it. |

### `lock_resource`

Acquire a named distributed lock. Fails if held by another agent.
Locks auto-expire after `ttl_secs` (default 300).

| Parameter | Required | Description |
|-----------|----------|-------------|
| `name` | yes | Resource name (e.g. file path) |
| `namespace` | no | |
| `ttl_secs` | no | Seconds until auto-expiry (default 300) |

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

| Tool | Session | Description |
|------|---------|-------------|
| `get_project` | yes | Get project metadata. |
| `update_project` | yes | Update project description. |
| `set_project_metadata` | yes | Set a metadata key-value pair on the project. |
| `get_project_overview` | yes | Get a comprehensive project overview. |
| `list_namespaces` | yes | List all registered namespaces for the project. |
| `poll_updates` | yes | Poll for recent domain events since a timestamp. |

### `get_project`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `include_summary` | no | `true` to add agent count, tasks by status, recent completions |

### `update_project`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `description` | yes | |
| `version` | no | Expected version for optimistic concurrency |

### `set_project_metadata`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `key` | yes | |
| `value` | yes | |

### `get_project_overview`

Get a comprehensive project overview: instructions, connected agents, active
tasks, and skills. Also available as HTTP GET `/bootstrap/{project}`.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |

### `poll_updates`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `since` | no | ISO 8601 timestamp. Returns events after this time |
| `limit` | no | Max events to return (default 50) |

---

## License

MIT
