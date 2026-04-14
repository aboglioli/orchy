# orchy

Multi-agent coordination server. Shared infrastructure for AI agents: task
board, shared memory, messaging, skill registry, and project context — exposed
as MCP tools over Streamable HTTP.

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

Three backends are supported. Only configure the section that matches your
`backend` value.

| Backend | Use case |
|---------|----------|
| `memory` | Development and testing. Data is lost on restart. |
| `sqlite` | Single-node deployments. Zero external dependencies. |
| `postgres` | Production. Requires a running PostgreSQL instance with the `pgvector` extension. |

For PostgreSQL, use the included `compose.yml`:

```bash
podman compose up -d   # or docker compose
```

### Skills

If `skills.dir` is set, orchy loads `.md` files from that directory on startup
and registers them as project skills. Skills are instructions and conventions
that agents receive when they connect.

### Embeddings

Optional. When configured, memory entries and context snapshots get vector
embeddings for semantic search. Any OpenAI-compatible embeddings API works
(OpenAI, Ollama, vLLM, etc.).

## Concepts

### Projects and namespaces

Every agent belongs to a **project** (e.g. `my-app`). Within a project,
resources are organized in **namespaces**: `/` is the root, `/backend` and
`/backend/auth` are scopes. Namespaces are hierarchical — reading from `/`
returns resources from all child namespaces.

### Agent lifecycle

1. Agent connects and calls `register_agent` with a project and optional roles.
2. If roles are omitted, orchy assigns roles based on pending task demand.
3. Agent calls `heartbeat` periodically to signal liveness.
4. On disconnect, claimed tasks are released back to pending.

### Task lifecycle

Tasks follow a state machine:

```
Pending --> Claimed --> InProgress --> Completed
   |           |            |
   v           v            v
Blocked    Failed       Failed
   |           |            |
   v           v            v
Cancelled  Cancelled    Cancelled
```

Tasks can be split into subtasks. When a task is split, the parent is blocked
and auto-completes when all subtasks finish. The auto-completion chains up the
full ancestor hierarchy.

Tasks return full context: when an agent claims a subtask, it receives the
ancestor chain (parent, grandparent, ...) and any sibling subtasks.

### Messages

Agents communicate through direct messages, role-targeted messages, or
broadcasts. Messages support threading via `reply_to` and have delivery
tracking (pending, delivered, read).

## MCP tools

All tools are exposed via the MCP protocol. Tools marked with **Session**
require a registered agent (call `register_agent` first). Session-bound tools
use the agent's project and namespace as defaults for scoping.

---

### Agent

#### `register_agent` 

Register this session as an agent. All subsequent tools are scoped to the
agent's project. If roles are empty, orchy assigns roles based on pending task
demand.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `project` | string | yes | Project identifier |
| `namespace` | string | no | Scope within the project (e.g. `"backend"`) |
| `roles` | string[] | no | Agent capabilities (e.g. `["coder", "reviewer"]`). Auto-assigned if omitted. |
| `description` | string | yes | What this agent is |
| `agent_id` | string | no | Resume a previous agent by its ID |
| `parent_id` | string | no | Create as a child of the given parent agent |

#### `list_agents` | Session

List all connected agents in the current project.

*No parameters.*

#### `change_roles` | Session

Change the roles of the current agent.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `roles` | string[] | yes | New role list |

#### `move_agent` | Session

Move the current agent to a different namespace within the same project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | yes | Target namespace |

#### `heartbeat` | Session

Send a heartbeat to signal liveness.

*No parameters.*

#### `disconnect` | Session

Disconnect the agent. Releases all claimed tasks back to pending.

*No parameters.*

---

### Tasks

#### `post_task` | Session

Create a new task.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | yes | Task title |
| `description` | string | yes | Task details |
| `namespace` | string | no | Scope. Defaults to session namespace. |
| `parent_id` | string | no | Parent task ID for creating a subtask |
| `priority` | string | no | `"low"`, `"normal"` (default), `"high"`, `"critical"` |
| `assigned_roles` | string[] | no | Roles that can claim this task |
| `depends_on` | string[] | no | Task IDs that must complete first |

#### `get_next_task` | Session

Get the next available task matching the agent's roles. Returns the task with
full context (ancestors and children).

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Filter by namespace |
| `role` | string | no | Filter by specific role. Defaults to agent's roles. |

#### `list_tasks` | Session

List tasks with optional filters.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Filter by namespace. Omit to see all project tasks. |
| `status` | string | no | Filter by status: `"pending"`, `"blocked"`, `"claimed"`, `"in_progress"`, `"completed"`, `"failed"`, `"cancelled"` |

#### `claim_task` | Session

Claim a task for the current agent. Returns the task with full context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID to claim |

#### `start_task` | Session

Transition a claimed task to in-progress. Returns the task with full context.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID to start |

#### `complete_task` | Session

Mark a task as completed.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `summary` | string | no | Result summary |

#### `fail_task` | Session

Mark a task as failed.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `reason` | string | no | Failure reason |

#### `assign_task` | Session

Assign or reassign a task to a specific agent.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `agent_id` | string | yes | Target agent ID |

#### `add_task_note` | Session

Add a timestamped note to a task.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `body` | string | yes | Note content |

#### `split_task` | Session

Split a task into subtasks. The parent is blocked and auto-completes when all
subtasks finish.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Parent task ID |
| `subtasks` | object[] | yes | List of subtask definitions (see below) |

Each subtask object:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `title` | string | yes | Subtask title |
| `description` | string | yes | Subtask details |
| `priority` | string | no | Priority level |
| `assigned_roles` | string[] | no | Roles for this subtask |
| `depends_on` | string[] | no | Dependency task IDs |

#### `replace_task` | Session

Cancel a task and create replacements.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID to cancel |
| `reason` | string | no | Cancellation reason |
| `replacements` | object[] | yes | Replacement task definitions (same schema as subtasks) |

#### `add_dependency` | Session

Add a dependency to a task. Blocks the task if the dependency is not completed.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `dependency_id` | string | yes | Task ID of the dependency |

#### `remove_dependency` | Session

Remove a dependency from a task. Unblocks the task if all remaining
dependencies are completed.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `dependency_id` | string | yes | Dependency to remove |

#### `move_task` | Session

Move a task to a different namespace.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `task_id` | string | yes | Task ID |
| `new_namespace` | string | yes | Target namespace |

---

### Memory

Shared key-value store with optional vector search. Agents use memory to share
decisions, context, and discoveries.

#### `write_memory` | Session

Write a key-value entry to shared memory. Supports optimistic concurrency via
version checking.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | yes | Entry key |
| `value` | string | yes | Entry value |
| `namespace` | string | no | Scope. Defaults to session namespace. |
| `version` | integer | no | Expected version for optimistic concurrency |

#### `read_memory` | Session

Read a memory entry by key.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | yes | Entry key |
| `namespace` | string | no | Scope. Defaults to root. |

#### `list_memory` | Session

List all memory entries.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Filter by namespace. Omit to see all. |

#### `search_memory` | Session

Search memory entries by semantic similarity.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Search query |
| `namespace` | string | no | Filter by namespace |
| `limit` | integer | no | Max results. Default 10. |

#### `delete_memory` | Session

Delete a memory entry.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | yes | Entry key |
| `namespace` | string | no | Scope. Defaults to session namespace. |

#### `move_memory` | Session

Move a memory entry to a different namespace.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | yes | Entry key |
| `namespace` | string | no | Source namespace. Defaults to session namespace. |
| `new_namespace` | string | yes | Target namespace |

---

### Messages

#### `send_message` | Session

Send a message to another agent, a role, or all agents.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `to` | string | yes | Target: agent ID, `"role:name"`, or `"broadcast"` |
| `body` | string | yes | Message content |
| `namespace` | string | no | Scope. Defaults to session namespace. |
| `reply_to` | string | no | Message ID this is replying to |

#### `check_mailbox` | Session

Check for pending messages.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Scope. Defaults to root. |

#### `mark_read`

Mark messages as read.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `message_ids` | string[] | yes | Message IDs to mark as read |

#### `check_sent_messages` | Session

Check the delivery and read status of messages the agent has sent.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Scope. Defaults to root. |

#### `list_conversation`

List the full conversation thread for a given message. Walks the `reply_to`
chain to find the root, then returns all messages in chronological order.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `message_id` | string | yes | Any message ID in the thread |
| `limit` | integer | no | Max messages to return (most recent N) |

---

### Context

Session context snapshots for continuity across agent sessions.

#### `save_context` | Session

Save a context snapshot before the session ends.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `summary` | string | yes | Session summary |
| `namespace` | string | no | Scope. Defaults to session namespace. |
| `metadata` | string | no | JSON string of key-value metadata |

#### `load_context` | Session

Load the most recent context snapshot.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | no | Agent ID. Defaults to the current agent. |

#### `list_contexts` | Session

List context snapshots.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | no | Filter by agent ID |
| `namespace` | string | no | Filter by namespace. Defaults to root. |

#### `search_contexts` | Session

Search context snapshots by semantic similarity.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Search query |
| `namespace` | string | no | Filter by namespace. Defaults to root. |
| `agent_id` | string | no | Filter by agent ID |
| `limit` | integer | no | Max results. Default 10. |

---

### Skills

Project-level instructions and conventions that agents receive on connect.

#### `write_skill` | Session

Create or update a skill.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | yes | Skill name (e.g. `"commit-conventions"`) |
| `description` | string | yes | Short description |
| `content` | string | yes | Full instruction text |
| `namespace` | string | no | Scope. Defaults to session namespace. |

#### `read_skill` | Session

Read a skill by name.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | yes | Skill name |
| `namespace` | string | no | Scope. Defaults to root. |

#### `list_skills` | Session

List skills.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Filter by namespace. Omit to see all. |
| `inherited` | boolean | no | Include skills from parent namespaces. Default false. |

#### `delete_skill` | Session

Delete a skill.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | yes | Skill name |
| `namespace` | string | no | Scope. Defaults to session namespace. |

#### `move_skill` | Session

Move a skill to a different namespace.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | yes | Skill name |
| `namespace` | string | no | Source namespace. Defaults to session namespace. |
| `new_namespace` | string | yes | Target namespace |

---

### Project

#### `get_project` | Session

Get the project metadata.

*No parameters.*

#### `update_project` | Session

Update the project description.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `description` | string | yes | New project description |

#### `add_project_note` | Session

Add a note to the project.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `body` | string | yes | Note content |

---

### Discovery

#### `list_namespaces` | Session

List all registered namespaces for the current project.

*No parameters.*

#### `get_bootstrap_prompt` | Session

Generate a full bootstrap prompt with all orchy instructions and project
skills. Use this for clients that don't support MCP instructions natively.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `namespace` | string | no | Scope. Defaults to root. |

## License

MIT
