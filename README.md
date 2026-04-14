# orchy

Multi-agent coordination server. Shared infrastructure for AI agents: task
board, shared memory, messaging, skill registry, documents, and project
context — exposed as 78 MCP tools over Streamable HTTP.

orchy is not an orchestrator. Agents bring the intelligence; orchy provides the
coordination layer and enforces the rules.

## Quick start

```bash
cargo run -p orchy-server
```

The MCP server starts at `http://127.0.0.1:3100/mcp`. Connect any
MCP-compatible client (Claude Code, Claude Desktop, etc.) to this URL.

A bootstrap prompt is available at `http://127.0.0.1:3100/bootstrap/<project>`
for clients that don't support MCP instructions natively.

## Configuration

All configuration lives in `config.toml` at the project root.

```toml
[server]
host = "127.0.0.1"
port = 3100
heartbeat_timeout_secs = 300    # seconds before an agent is marked as timed out

[store]
backend = "sqlite"              # "sqlite", "postgres", or "memory"

[store.sqlite]
path = "orchy.db"               # file path for the SQLite database

# [store.postgres]
# url = "postgres://orchy:orchy@localhost:5432/orchy"

# [skills]
# dir = "skills"                # directory to load skill files from on startup

# [embeddings]
# provider = "openai"
#
# [embeddings.openai]
# url = "https://api.openai.com/v1/embeddings"
# model = "text-embedding-3-small"
# dimensions = 1536
```

### Server

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `host` | string | `"127.0.0.1"` | Bind address |
| `port` | integer | `3100` | Bind port |
| `heartbeat_timeout_secs` | integer | `300` | Seconds of inactivity before an agent is disconnected |

### Store

| Backend | Use case |
|---------|----------|
| `memory` | Development and testing. Data is lost on restart. |
| `sqlite` | Single-node deployments. Zero external dependencies. |
| `postgres` | Production. Requires PostgreSQL with `pgvector` extension. |

For PostgreSQL, use the included `compose.yml`:

```bash
podman compose up -d
```

### Skills

If `skills.dir` is set, orchy loads `.md` files from that directory on startup
and registers them as project skills.

### Embeddings

Optional. When configured, memory entries, documents, and context snapshots get
vector embeddings for semantic search. Any OpenAI-compatible embeddings API
works (OpenAI, Ollama, vLLM, etc.).

## Concepts

### Projects and namespaces

Every agent belongs to a **project** (e.g. `my-app`). Within a project,
resources are organized in **namespaces**: `/` is the root, `/backend` and
`/backend/auth` are scopes. Namespaces are hierarchical — reading from `/`
returns resources from all child namespaces. Namespaces are auto-created on
first use.

### Agent lifecycle

1. Agent connects and calls `register_agent` with a project and optional roles.
2. If roles are omitted, orchy assigns roles based on pending task demand.
3. Agent calls `heartbeat` periodically to signal liveness.
4. On disconnect (or timeout), claimed tasks are released, resource locks are
   freed, watchers are removed, and pending reviews are unassigned.

### Task lifecycle

```
Pending --> Claimed --> InProgress --> Completed
   |           |            |
   v           v            v
Blocked     Failed       Failed
   |           |            |
   v           v            v
Cancelled  Cancelled    Cancelled
```

Tasks support hierarchy: `split_task` creates subtasks under a parent. The
parent blocks and auto-completes when all subtasks finish. Auto-completion
chains up the full ancestor hierarchy.

Tasks return full context: when an agent claims a subtask, it receives the
ancestor chain (parent, grandparent, ...) and sibling subtasks.

### Documents

Long-form versioned content with hierarchical paths (e.g. `specs/auth-design`).
Supports tags, semantic search, and namespace scoping.

### Resource locking

TTL-based locking for any named resource (files, deployments, refactoring
scopes). Locks auto-expire and are released on agent disconnect.

### Project links

Projects can link to other projects to share resources (skills, memory). Use a
shared "global" project as a common resource pool by linking other projects to
it.

### Event log

Every state change is recorded as a semantic domain event in an append-only
event log. Events can be queried with `poll_updates` for change tracking.

### Reviews and watchers

Agents can request code reviews on tasks and watch tasks for updates. Pending
reviews are cleaned up on agent disconnect.

## MCP tools

All tools are exposed via the MCP protocol. Tools marked with **Session**
require a registered agent (`register_agent` first).

---

### Agent

#### `register_agent`

Register as an agent. Required before any other tool.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project` | string | yes | Project identifier |
| `namespace` | string | no | Scope within the project |
| `roles` | string[] | no | Agent capabilities. Auto-assigned if omitted. |
| `description` | string | yes | What this agent is |
| `agent_id` | string | no | Resume a previous agent session |
| `parent_id` | string | no | Create as a child of this parent agent |

#### `list_agents` | Session

List all connected agents in the current project.

#### `change_roles` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `roles` | string[] | yes |

#### `move_agent` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | yes |

#### `heartbeat` | Session

Signal liveness.

#### `disconnect` | Session

Disconnect and release all held resources (tasks, locks, watchers, reviews).

---

### Tasks

#### `post_task` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | yes | |
| `description` | string | yes | |
| `namespace` | string | no | Defaults to session namespace |
| `parent_id` | string | no | Parent task ID for subtask |
| `priority` | string | no | low, normal (default), high, critical |
| `assigned_roles` | string[] | no | Roles that can claim. Empty = any. |
| `depends_on` | string[] | no | Task IDs that must complete first |

#### `get_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `get_next_task` | Session

Claim the next available task matching your roles. Returns full context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Filter by namespace |
| `role` | string | no | Defaults to all agent roles |

#### `list_tasks` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Omit to see all |
| `status` | string | no | pending, blocked, claimed, in_progress, completed, failed, cancelled |

#### `claim_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `start_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `complete_task` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | |
| `summary` | string | no | Visible to other agents and parent tasks |

#### `fail_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `reason` | string | no |

#### `release_task` | Session

Release a claimed task back to pending.

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `assign_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `agent_id` | string | yes |

#### `delegate_task` | Session

Create a subtask while keeping the parent claimed.

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `title` | string | yes |
| `description` | string | yes |
| `priority` | string | no |
| `assigned_roles` | string[] | no |

#### `add_task_note` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `body` | string | yes |

#### `split_task` | Session

Split a task into subtasks. Parent blocks and auto-completes when all finish.

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `subtasks` | object[] | yes |

Each subtask: `title` (required), `description` (required), `priority`, `assigned_roles`, `depends_on`.

#### `replace_task` | Session

Cancel a task and create replacements.

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `reason` | string | no |
| `replacements` | object[] | yes |

#### `merge_tasks` | Session

Merge multiple tasks into one.

| Parameter | Type | Required |
|-----------|------|----------|
| `task_ids` | string[] | yes |
| `title` | string | yes |
| `description` | string | yes |

#### `list_subtasks` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `add_dependency` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `dependency_id` | string | yes |

#### `remove_dependency` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `dependency_id` | string | yes |

#### `tag_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `tag` | string | yes |

#### `untag_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `tag` | string | yes |

#### `list_tags` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | no |

#### `move_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |
| `new_namespace` | string | yes |

#### `watch_task` | Session

Watch a task for updates.

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `unwatch_task` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

#### `request_review` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | |
| `reviewer_agent` | string | no | Specific reviewer agent ID |
| `reviewer_role` | string | no | Target role (e.g. "reviewer") |

#### `resolve_review` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `review_id` | string | yes |
| `approved` | boolean | yes |
| `comments` | string | no |

#### `list_reviews` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `task_id` | string | yes |

---

### Memory

Shared key-value store with optimistic concurrency via version checking.

#### `write_memory` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | yes | |
| `value` | string | yes | |
| `namespace` | string | no | |
| `version` | integer | no | Expected version for optimistic concurrency |

#### `append_memory` | Session

Append to an existing entry's value.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | yes | |
| `value` | string | yes | Text to append |
| `namespace` | string | no | |
| `separator` | string | no | Defaults to `"\n"` |

#### `read_memory` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `key` | string | yes |
| `namespace` | string | no |

#### `list_memory` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | no |

#### `search_memory` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `query` | string | yes |
| `namespace` | string | no |
| `limit` | integer | no |

#### `delete_memory` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `key` | string | yes |
| `namespace` | string | no |

#### `move_memory` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `key` | string | yes |
| `namespace` | string | no |
| `new_namespace` | string | yes |

---

### Messages

#### `send_message` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `to` | string | yes | Agent UUID, `"role:name"`, or `"broadcast"` |
| `body` | string | yes | |
| `namespace` | string | no | |
| `reply_to` | string | no | Message ID to thread |

#### `check_mailbox` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | no |

#### `mark_read`

| Parameter | Type | Required |
|-----------|------|----------|
| `message_ids` | string[] | yes |

#### `check_sent_messages` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | no |

#### `list_conversation`

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `message_id` | string | yes | Any message in the thread |
| `limit` | integer | no | Most recent N |

---

### Documents

Versioned long-form content with hierarchical paths.

#### `write_document` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | yes | Hierarchical path (e.g. `"specs/auth-design"`) |
| `title` | string | yes | |
| `content` | string | yes | |
| `namespace` | string | no | |
| `tags` | string[] | no | |
| `version` | integer | no | Expected version for optimistic concurrency |

#### `read_document` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `path` | string | yes |
| `namespace` | string | no |

#### `list_documents` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | no |
| `tag` | string | no |
| `path_prefix` | string | no |

#### `search_documents` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `query` | string | yes |
| `namespace` | string | no |
| `limit` | integer | no |

#### `delete_document` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `path` | string | yes |
| `namespace` | string | no |

#### `move_document` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `path` | string | yes |
| `namespace` | string | no |
| `new_namespace` | string | yes |

#### `rename_document` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `path` | string | yes |
| `namespace` | string | no |
| `new_path` | string | yes |

#### `tag_document` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `path` | string | yes |
| `namespace` | string | no |
| `tag` | string | yes |

---

### Context

Session snapshots for continuity across agent sessions.

#### `save_context` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `summary` | string | yes | Session summary |
| `namespace` | string | no | |
| `metadata` | string | no | JSON string of key-value pairs |

#### `load_context` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | no | Defaults to current agent |

#### `list_contexts` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `agent_id` | string | no |
| `namespace` | string | no |

#### `search_contexts` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `query` | string | yes |
| `namespace` | string | no |
| `agent_id` | string | no |
| `limit` | integer | no |

---

### Skills

Project-level instructions and conventions that agents receive on connect.

#### `write_skill` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `name` | string | yes |
| `description` | string | yes |
| `content` | string | yes |
| `namespace` | string | no |

#### `read_skill` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `name` | string | yes |
| `namespace` | string | no |

#### `list_skills` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | |
| `inherited` | boolean | no | Include parent namespace skills |

#### `delete_skill` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `name` | string | yes |
| `namespace` | string | no |

#### `move_skill` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `name` | string | yes |
| `namespace` | string | no |
| `new_namespace` | string | yes |

---

### Resource Locking

TTL-based locking for any named resource.

#### `lock_resource` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | yes | Resource name (e.g. file path) |
| `namespace` | string | no | |
| `ttl_secs` | integer | no | Auto-expiry. Default 300. |

#### `unlock_resource` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `name` | string | yes |
| `namespace` | string | no |

#### `check_lock` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `name` | string | yes |
| `namespace` | string | no |

---

### Project

#### `get_project` | Session

#### `update_project` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `description` | string | yes |

#### `add_project_note` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `body` | string | yes |

#### `get_project_summary` | Session

Overview of connected agents, task counts, and recent activity.

#### `get_agent_workload` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | no | Defaults to current agent |

---

### Project Links

Cross-project resource sharing.

#### `link_project` | Session

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_project` | string | yes | Project to import from |
| `resource_types` | string[] | yes | `"skills"`, `"memory"` |

#### `unlink_project` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `source_project` | string | yes |

#### `list_project_links` | Session

#### `import_skill` | Session

Import a skill from a linked project.

| Parameter | Type | Required |
|-----------|------|----------|
| `source_project` | string | yes |
| `name` | string | yes |
| `source_namespace` | string | no |

#### `import_memory` | Session

Import a memory entry from a linked project.

| Parameter | Type | Required |
|-----------|------|----------|
| `source_project` | string | yes |
| `key` | string | yes |
| `source_namespace` | string | no |

---

### Discovery

#### `list_namespaces` | Session

#### `get_bootstrap_prompt` | Session

| Parameter | Type | Required |
|-----------|------|----------|
| `namespace` | string | no |

#### `poll_updates` | Session

Poll the event log for recent changes.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `since` | string | no | ISO 8601 timestamp |
| `limit` | integer | no | Max events to return |

## License

MIT
