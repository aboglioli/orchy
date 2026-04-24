# Orchy

Multi-agent coordination server. Shared infrastructure for AI agents: task
board, unified knowledge base, messaging, resource locking, graph edges, and
project context — exposed as **71** MCP tools over Streamable HTTP and as a
stateless **CLI** (`orchy`) for agents without MCP support.

orchy is not an orchestrator. Agents bring the intelligence; orchy provides
the coordination layer and enforces the rules.

## Quick start

```bash
cargo run -p orchy-server   # MCP + REST API
cargo run -p orchy-cli      # CLI binary (requires a running server)
```

MCP server at `http://127.0.0.1:3100/mcp`. Bootstrap prompt at
`http://127.0.0.1:3100/bootstrap/<project>`.

CLI quickstart:
```bash
# Configure once (or use env vars: ORCHY_URL, ORCHY_API_KEY, ORCHY_PROJECT, ORCHY_ALIAS)
cat > ~/.orchy/config.toml <<EOF
url      = "http://localhost:3100"
api_key  = "your-key"
project  = "myproject"
alias    = "coder-1"
EOF

orchy bootstrap              # agent briefing: inbox, tasks, skills, handoff context
orchy task list --json       # machine-readable output for agents
orchy task claim <id>
orchy knowledge write auth/decision --kind decision --title "..." --content "..."
```

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

### Entity Relationships

Orchy's four main entities — **Agents**, **Tasks**, **Knowledge**, and **Messages** — are connected through two mechanisms:

1. **Direct references** (fields on entities) — runtime state, ownership, hierarchy
2. **Graph edges** (semantic links) — relationships, provenance, dependencies

#### Direct References

| Entity | Field | Points To | Purpose |
|--------|-------|-----------|---------|
| Task | `assigned_to` | Agent | Runtime task assignment (who is working on it) |
| Message | `from_agent` | Agent | Sender |
| Message | `reply_to` | Message | Threading |

`Task.parent_id`, `Task.depends_on`, and `Knowledge.agent_id` were migrated to
graph edges (`spawns`, `depends_on`, `owned_by`) in Phase 2/3 of the graph rollout.
They no longer exist as columns.

#### Graph Edges (Semantic Links)

Edges model relationships that cross entity boundaries. They are:
- **Typed** (15 relation types)
- **Directed** (A → B)
- **Traversable** (multi-hop graph queries)
- **Temporal** (point-in-time snapshots)

**Resource kinds that can be linked:** `task`, `knowledge`, `agent`

**Messages cannot be graph nodes** — use message threading instead.

---

### Relation Types

#### Task ↔ Task (Work Dependencies)

| Relation | Direction | Meaning | Auto-Created? |
|----------|-----------|---------|---------------|
| `depends_on` | Task → Task | Must complete before claiming | `add_dependency`, `post_task` |
| `spawns` | Task → Task | Parent created child | `split_task`, `delegate_task`, `post_task(parent_id)` |
| `supersedes` | Task → Task | Replacement for obsolete task | `replace_task` |
| `merged_from` | Task ← Task | Merged from sources | `merge_tasks` |

#### Task ↔ Knowledge (Work Products)

| Relation | Direction | Meaning | Auto-Created? |
|----------|-----------|---------|---------------|
| `produces` | Task → Knowledge | Work produced knowledge | `write_knowledge(task_id=...)` |
| `implements` | Task → Knowledge | Task executes plan/decision | Manual via `add_edge` |
| `derived_from` | Knowledge → Task | Knowledge came from work | Manual via `add_edge` |

#### Knowledge ↔ Knowledge (Information Flow)

| Relation | Direction | Meaning |
|----------|-----------|---------|
| `summarizes` | Knowledge → Knowledge | Summary covers sources |
| `merged_from` | Knowledge ← Knowledge | Consolidated from sources |
| `supersedes` | Knowledge → Knowledge | New version replaces old |
| `derived_from` | Knowledge → Knowledge | Chained reasoning |
| `invalidates` | Knowledge → Knowledge | This invalidates that fact |
| `confirms` | Knowledge → Knowledge | Evidence confirms claim |
| `supported_by` | Knowledge ← Knowledge | Claim supported by evidence |
| `contradicted_by` | Knowledge ← Knowledge | Fact contradicted by other |
| `related_to` | Knowledge ↔ Knowledge | General relationship |

#### Agent ↔ Task/Knowledge (Ownership)

| Relation | Direction | Meaning |
|----------|-----------|---------|
| `owned_by` | Task/Knowledge → Agent | Semantic ownership/responsibility |
| `reviewed_by` | Task/Knowledge → Agent | Reviewed or approved by |

**Note:** `assigned_to` (task field) is **runtime assignment** — who is actively working on the task. `owned_by` edge is **semantic ownership** — who is responsible for the resource long-term. A task can be `assigned_to` agent A while `owned_by` agent B.

---

### Auto-Created Edges

The system automatically creates graph edges during these operations:

```rust
// Task produces knowledge
write_knowledge(task_id="task-123", ...)     // Task →[produces]→ Knowledge

// Split task into subtasks
split_task(parent_id="task-123", subtasks=[...])   // Parent →[spawns]→ Children

// Delegate subtask (non-blocking)
delegate_task(task_id="task-123", ...)       // Parent →[spawns]→ Subtask

// Replace obsolete task
replace_task(task_id="task-123", replacements=[...])   // Replacement →[supersedes]→ Original

// Merge multiple tasks
merge_tasks(task_ids=["a", "b", "c"], ...)    // Merged ←[merged_from]— Sources

// Add dependency
add_dependency(task_id="task-a", dependency_id="task-b")   // TaskA →[depends_on]→ TaskB

// Create subtask via parent
post_task(parent_id="task-123", ...)         // Parent →[spawns]→ Child

// Create task with dependencies
post_task(depends_on=["task-a", "task-b"], ...)   // Task →[depends_on]→ Deps
```

**Manual edges** for semantic relationships not covered above:
```rust
// Task implements a plan
add_edge(from_kind="task", from_id="impl-123",
         to_kind="knowledge", to_id="plan-456",
         rel_type="implements")

// Knowledge derived from task
add_edge(from_kind="knowledge", from_id="decision-789",
         to_kind="task", to_id="research-001",
         rel_type="derived_from")

// Resource ownership
add_edge(from_kind="task", from_id="task-123",
         to_kind="agent", to_id="agent-456",
         rel_type="owned_by")

// Evidence chain
add_edge(from_kind="knowledge", from_id="evidence-001",
         to_kind="knowledge", to_id="claim-002",
         rel_type="supported_by")
```

---

### Traversal and Context Tools

#### Materializing Referenced Nodes

Several tools can load referenced resources in a single call:

| Tool | Loads | Description |
|------|-------|-------------|
| `get_task` | Task + ancestors + children + dependencies (optional) + knowledge (optional) | Full task context |
| `assemble_context` | Structured context from graph | Curated context for AI |

**`get_task` materialization flags:**
```rust
get_task(
    task_id: "task-123",
    include_dependencies: true,      // Load full Task objects for depends_on IDs
    include_knowledge: true,         // Load linked knowledge entries
    knowledge_limit: 5,              // Max knowledge entries
    knowledge_kind: "decision",      // Filter by kind
    knowledge_tag: "critical",       // Filter by tag
    knowledge_content_limit: 500     // Truncate content
)
```

**`assemble_context`** — curated context for AI consumption:
```rust
assemble_context(kind: "task", id: "task-123", max_tokens: 4000)
// Returns: core_facts, open_dependencies, relevant_decisions, recent_changes, risk_flags
```

---

### Knowledge

The organization's persistent memory. Agents don't retain state between
sessions — every insight, decision, or convention must be written to
knowledge or it's gone. Knowledge entries are the org's wiki.

Each entry has a `kind`, a hierarchical `path`, `title`, `content`, `tags`,
and a `version` for optimistic concurrency control. Two agents writing the
same entry concurrently will get a conflict error on the stale write.

**Addressing:** entries are looked up by `(project, namespace, path)`. Paths
use forward slashes for sub-topics: `db-choice`, `auth/jwt-strategy`,
`api/error-handling`. The path is the canonical identifier — write to it
to create or update.

**Kinds** categorize the intent of an entry:

| Kind | Use for |
|------|---------|
| `note` | General observations and records |
| `decision` | Choices made with rationale ("we chose RS256 because…") |
| `discovery` | Things found or learned — gotchas, constraints, findings |
| `pattern` | Recurring approaches or conventions agents should follow |
| `context` | Session handoff snapshots — what you were doing, what's left |
| `document` | Long-form specs, architecture decisions, analysis |
| `config` | Configuration or setup information |
| `reference` | External references or links |
| `plan` | Strategies, roadmaps, implementation approaches |
| `log` | Activity or change log entries |
| `skill` | Instructions agents must follow, inherited through namespace hierarchy |
| `overview` | Project summary surfaced in the bootstrap prompt |
| `summary` | Compact synthesized output: task summaries, agent rollups |
| `report` | Richer completion artifacts: post-task writeups, implementation reports |

**Skills** (kind=`skill`) are inherited through the namespace hierarchy: an
agent in `/backend/auth` sees skills from `/`, `/backend`, and
`/backend/auth`. Child namespaces override parent skills with the same path.

**Search** supports both full-text (FTS) and semantic (embedding-based)
queries via `search_knowledge`. The hybrid scoring blends embedding similarity
with keyword overlap and boosts entries linked to the query's anchor resource
via the edge graph.

**Metadata** is a free-form `HashMap<String, String>` attached to any entry.
Useful for tagging structured facts (e.g. `{"status": "superseded",
"ticket": "ORG-42"}`).

### Tasks

The work board. Tasks are the unit of work agents claim and execute.

```
Pending → Claimed → InProgress → Completed
   │         │          │
   └─────────┴──────────┴──────→ Failed → Cancelled
                                          ↑
                            Blocked ───────┘ (also Pending)
```

**Assignment** is exclusive: only one agent holds a task at a time via the
`assigned_to` field. Claiming reserves it; starting moves it to in-progress;
completing, failing, or cancelling terminates it. Tasks become stale after
inactivity (`stale_after_secs`), making them claimable by other agents.
Use `touch_task` to keep long-running work alive.

**Ownership** (via `owned_by` edge) is semantic — who is responsible for the
resource. This survives task reassignment and persists after completion.

**Hierarchy:** tasks can have a `parent_id`. `split_task` blocks the parent
and creates subtasks; the parent auto-completes when all subtasks finish or
are cancelled. `delegate_task` creates a subtask without blocking the parent.
Both operations auto-create `spawns` edges.

**Dependencies:** `depends_on` is a list of task IDs that must complete before
this task can be claimed. A task with unmet dependencies starts as blocked and
unblocks automatically when its last dependency completes. Failed dependencies
send a system notification to the waiting task's last assignee. Dependencies
are also represented as `depends_on` edges in the graph.

**Priority:** `low`, `normal` (default), `high`, `critical`. `get_next_task`
returns the highest-priority available task matching the agent's roles.

**Roles:** `assigned_roles` restricts which agents can claim a task. Empty
means any agent can claim it. orchy auto-assigns agent roles from the set of
pending role requirements on startup.

**Tags** are free-form labels for grouping and filtering.

### Messages

Real-time communication between agents — the coordination bus.

Messages are **immutable** once sent. You reply to a message (creating a
thread), but you cannot edit or delete one.

**Addressing modes:**

| Target | Syntax | Who receives it |
|--------|--------|-----------------|
| Direct | `@alias` or agent UUID | That agent only |
| Role | `role:reviewer` | All agents with that role |
| Namespace | `ns:/backend` | All agents in that namespace |
| User | `user:<uuid>` | All agents owned by that user |
| Broadcast | `broadcast` | All agents in the project except the sender |

Role, namespace, user, and broadcast messages support optional **claim/unclaim**
semantics — claimed messages are hidden from sibling inboxes.

**Threading:** any message can set `reply_to` pointing at another message ID.
`list_conversation` walks the full chain up and down from any message in the
thread.

**Delivery tracking:** messages start as `pending`, move to `delivered` when
the recipient polls their mailbox, and to `read` when explicitly marked.
For role and broadcast messages, each recipient is tracked independently via
`message_receipts`. `check_sent_messages` shows per-message delivery status.

**System messages:** orchy itself sends messages to agent mailboxes for
dependency failure notifications. Agents should poll `check_mailbox` regularly
or use `poll_updates` to catch these.

**Note:** Messages are not part of the graph edge system. Use message
centeric tools (`send_message`, `check_mailbox`, `list_conversation`) instead.

### Agent lifecycle

1. `register_agent(alias, project, description)` — roles auto-assigned from task demand. Org derived from API key.
2. `get_agent_context` — everything in one call: agent info, project, inbox, pending tasks, skills, handoff context
3. `get_next_task` — claim or peek the next available task matching your roles
4. `heartbeat` every ~30s; on timeout: tasks become stale, locks freed

### Resource locking

TTL-based locking for any named resource. Auto-expires and cleaned up
on agent disconnect.

### Project links

Projects can link to other projects to share knowledge entries.

### Event log

Every state change is recorded as a semantic domain event. Query with
`poll_updates`.

## REST Graph Endpoints

Graph operations are available under `/graph/` for REST clients and the CLI.
Org is derived from the API key (Bearer token).

```
POST   /graph/edges                    add_edge
DELETE /graph/edges/{edge_id}          remove_edge
GET    /graph/relations                query_relations (query params: anchor_kind, anchor_id, rel_types, direction, max_depth, as_of)
GET    /graph/edges                    list edges (admin/debug)
POST   /graph/context                  assemble_context
```

`GET /tasks/{id}`, `GET /knowledge/{*path}`, and `GET /agents/{id}` also accept
`rel_types`, `direction`, and `max_depth` query params to inline an
`EntityNeighborhood` in the response without a separate call.

## CLI

`orchy` is a stateless CLI client targeting agents without MCP support (e.g. pi coding agent, Codex). Every invocation is a single REST request — no session, no heartbeat required.

**Config resolution** (lowest → highest priority):
1. `~/.orchy/config.toml` — global defaults
2. `.orchy.toml` — repo-local config (walked up from cwd, like mise)
3. Env vars: `ORCHY_URL`, `ORCHY_API_KEY`, `ORCHY_PROJECT`, `ORCHY_NAMESPACE`, `ORCHY_ALIAS`
4. Per-call flags: `--url`, `--api-key`, `--project`, `--namespace`, `--agent`

**Output:** text by default, `--json` on every command for machine-parseable output.

**Key commands:**
```bash
orchy bootstrap --json          # full agent briefing (inbox, tasks, skills, handoff)
orchy task list --json
orchy task claim <id>
orchy task complete <id> --summary "..."
orchy knowledge write <path> --kind decision --title "..." --content "..."
orchy knowledge read <path> --json
orchy message send --to broadcast --body "..."
orchy edge add --from-kind task --from-id <id> --to-kind knowledge --to-id <id> --rel-type produces
orchy edge query --anchor-kind task --anchor-id <id> --json
orchy lock acquire myfile.rs --ttl 120
```

Full command reference: `orchy --help` / `orchy <domain> --help`.

## MCP Tools

Authoritative definitions: `crates/orchy-server/src/mcp/tools/` and
`crates/orchy-server/src/mcp/params.rs`. A running server exposes the current
set via MCP `list_tools`.

**Session** — `yes` means call `register_agent` first. **no** — callable
without registration. **partial** — `list_agents` only: pass `project`,
or register to use the session project.

Tools that do not require registration: `register_agent`, `session_status`,
`list_knowledge_types`, and `list_agents` when `project` is passed.

---

## Agent

| Tool | Session | Description |
|------|---------|-------------|
| `register_agent` | no | Register as an agent. Required before almost every other tool. |
| `session_status` | no | Check whether this MCP session is bound to an orchy agent. |
| `get_agent_context` | yes | Get everything in one call: agent info, project, inbox, pending tasks, skills, handoff context. |
| `list_agents` | partial | List agents in a project. Works before registration if `project` is passed. |
| `change_roles` | yes | Change the roles of the session agent. |
| `heartbeat` | yes | Send a heartbeat to signal liveness. |
| `rename_alias` | yes | Change the agent's alias. Internal UUID is unchanged. |
| `switch_context` | yes | Switch agent to a different project, namespace, or both within the same org. |

### `register_agent`

Register as an agent. Required before almost every other tool. Identity is
alias-based — re-registering with the same `(org, project, alias)` resumes the
existing agent. Org is derived from the API key. Roles are auto-assigned from
pending task demand if omitted.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `alias` | yes | Agent identity (lowercase alphanumeric + hyphens, 2-32 chars) |
| `project` | yes | Project identifier |
| `description` | yes | What this agent is |
| `namespace` | no | Scope within project |
| `roles` | no | Capabilities. Auto-assigned from task demand if omitted |
| `agent_type` | no | Informative only (e.g. `claude-code`, `opencode`, `pi`). Not part of identity. |
| `metadata` | no | Key-value pairs attached to the agent record |

### `session_status`

Check whether this MCP session is bound to an orchy agent, and how to resume
after an orchy or MCP transport restart. Does not require registration. Call
after the client has reconnected (new MCP initialize) if tools failed with
session errors or you are unsure whether you still need `register_agent`.

### `get_agent_context`

Get everything you need in one call: your agent info, project metadata,
inbox messages, pending tasks matching your roles, skills, and handoff
context from previous sessions. Call this after `register_agent` to
bootstrap quickly. No parameters — uses the session agent.

### `list_agents`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | no | Required if not registered yet |

### `change_roles`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `roles` | yes | New role list (replaces existing) |

### `switch_context`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | no | Target project. Resets namespace to root unless namespace also provided. |
| `namespace` | no | Target namespace within the project. |

At least one of `project` or `namespace` is required. Switching projects
releases claimed tasks and locks in the old project.

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

Create a task. Use `split_task` for subtasks with hierarchy, `add_dependency`
for dependencies, or `delegate_task` for non-blocking subtasks.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `title` | yes | Task title |
| `description` | yes | Task description |
| `acceptance_criteria` | no | Definition of done for this task |
| `namespace` | no | Defaults to session namespace |
| `priority` | no | `low`, `normal` (default), `high`, `critical` |
| `assigned_roles` | no | Roles that can claim. Empty = any role |

### `delegate_task`

Create a subtask under a claimed/in-progress task without blocking the parent.
Unlike `split_task`, the parent keeps its status. Creates `spawns` edge.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Parent task to delegate from (stays claimed) |
| `title` | yes | Subtask title |
| `description` | yes | Subtask description |
| `acceptance_criteria` | no | Definition of done |
| `priority` | no | Defaults to parent priority |
| `assigned_roles` | no | Roles that can claim the subtask |

### `split_task`

Split a task into subtasks. The parent task is blocked and will auto-complete
when all subtasks finish. Agents should work on subtasks directly, not the
parent. Creates `spawns` edges from parent to each subtask.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Parent task to split |
| `subtasks` | yes | Array of subtask definitions |
| `subtasks[].title` | yes | |
| `subtasks[].description` | yes | |
| `subtasks[].acceptance_criteria` | no | Definition of done for this subtask |
| `subtasks[].priority` | no | `low`, `normal` (default), `high`, `critical` |
| `subtasks[].assigned_roles` | no | |
| `subtasks[].depends_on` | no | |

### `replace_task`

Cancel a task and create replacements that inherit the original's parent (if any).
Creates `supersedes` edges from each replacement to the original task.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | Task to cancel |
| `reason` | no | Reason for replacement |
| `replacements` | yes | Array of subtask definitions (same structure as `split_task`) |

### `merge_tasks`

Merge multiple tasks into one. Source tasks must be pending, blocked, or claimed.
They are cancelled and a new consolidated task is created with the highest
priority, combined roles, combined dependencies, and collected notes.
Creates `merged_from` edges from new task to each source.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_ids` | yes | At least 2 task UUIDs. Must be pending, blocked, or claimed |
| `title` | yes | Title for the merged task |
| `description` | yes | Description for the merged task |
| `acceptance_criteria` | no | Definition of done for the merged task |

---

## Tasks — Lifecycle

| Tool | Session | Description |
|------|---------|-------------|
| `get_next_task` | yes | Claim or peek the next available task matching your roles. |
| `get_task` | yes | Get a task by ID with full context (ancestors + children + optional materialization). |
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
| `touch_task` | yes | Update last_activity_at to prevent staleness. Call periodically for long-running work. |
| `archive_task` | yes | Archive a completed/failed/cancelled task. Hidden from listings. |
| `unarchive_task` | yes | Restore an archived task. |

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
| `project` | no | Override session project to query another project |
| `after` | no | Cursor for pagination (task ID from `next_cursor` of previous page) |
| `limit` | no | Max items per page |

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

### `get_task`

Get a task by ID with full context (ancestors + children).
Can materialize referenced dependencies and linked knowledge entries.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `include_dependencies` | no | When true, fetches the full dependency task list |
| `include_knowledge` | no | When true, fetches knowledge entries linked to this task via edges |
| `knowledge_limit` | no | Max knowledge entries to return (default 5) |
| `knowledge_kind` | no | Filter linked knowledge by kind |
| `knowledge_tag` | no | Filter linked knowledge by tag |
| `knowledge_content_limit` | no | Max characters of content per knowledge entry (default 500) |

### `update_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `title` | no | |
| `description` | no | |
| `acceptance_criteria` | no | |
| `priority` | no | `low`, `normal`, `high`, `critical` |

### `assign_task`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `task_id` | yes | |
| `agent` | yes | Target agent UUID |

---

## Tasks — Dependencies

| Tool | Session | Description |
|------|---------|-------------|
| `add_dependency` | yes | Add a dependency. Task blocks until dependency completes. Creates `depends_on` edge. |
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
| `tag_task` | yes | Add a tag to a task. |
| `untag_task` | yes | Remove a tag from a task. |
| `list_tags` | yes | List all unique tags used across tasks in the project. |
| `move_task` | yes | Move a task to a different namespace within the same project. |

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

## Messages

| Tool | Session | Description |
|------|---------|-------------|
| `send_message` | yes | Send a message to an agent, role, namespace, user, or broadcast. |
| `check_mailbox` | yes | Check your mailbox for incoming messages. |
| `check_sent_messages` | yes | List messages you have sent, with delivery and read status. |
| `mark_read` | yes | Mark messages as read by their IDs. |
| `list_conversation` | yes | Get the full conversation thread for a message ID. |
| `claim_message` | yes | Claim a logical (role/ns/broadcast/user) message for exclusive handling. |
| `unclaim_message` | yes | Release a previously claimed message. |

### `send_message`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `to` | yes | `@alias`, agent UUID, `role:name`, `ns:/path`, `user:<uuid>`, or `broadcast` |
| `body` | yes | Message body |
| `namespace` | no | |
| `reply_to` | no | Message ID to thread the message |
| `refs` | no | Array of `{kind, id, display?}` resource pointers. Context hints for the recipient — not graph edges. |

### `check_mailbox` / `check_sent_messages`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |
| `project` | no | Override session project |
| `after` | no | Cursor for pagination (message ID from `next_cursor`) |
| `limit` | no | Max items per page |

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
| `write_knowledge` | yes | Create or update a knowledge entry by path. Can auto-create `produces` edge. |
| `read_knowledge` | yes | Read a knowledge entry by path. |
| `list_knowledge` | yes | List knowledge entries with optional filters. |
| `search_knowledge` | yes | Search knowledge entries by semantic similarity. Supports proximity boosts via graph. |
| `delete_knowledge` | yes | Delete a knowledge entry by path. |
| `append_knowledge` | yes | Append text to a knowledge entry. Creates if it doesn't exist. |
| `patch_knowledge_metadata` | yes | Merge or remove metadata without changing content. |
| `move_knowledge` | yes | Move a knowledge entry to a different namespace. |
| `rename_knowledge` | yes | Rename a knowledge entry's path. |
| `change_knowledge_kind` | yes | Change the kind of an existing entry. |
| `tag_knowledge` | yes | Add a tag to a knowledge entry. |
| `untag_knowledge` | yes | Remove a tag from a knowledge entry. |
| `import_knowledge` | yes | Import a knowledge entry from a linked project. |
| `promote_knowledge` | yes | Promote a decision/discovery/pattern into a reusable skill. |
| `consolidate_knowledge` | yes | Merge multiple entries into one, deleting sources. |
| `archive_knowledge` | yes | Archive a knowledge entry. Hidden from listings. |
| `unarchive_knowledge` | yes | Restore an archived knowledge entry. |
| `assemble_context` | yes | Assemble rich structured context for a resource by traversing the edge graph. |

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
| `task_id` | no | When provided, auto-creates a Task→Knowledge `produces` edge |

### `read_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `namespace` | no | Defaults to root namespace |
| `project` | no | Override session project |

### `list_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `namespace` | no | |
| `kind` | no | Filter by kind |
| `tag` | no | Filter by tag |
| `path_prefix` | no | Filter by path prefix |
| `agent` | no | Filter by author UUID |
| `orphaned` | no | `true` — entries with no incoming `produces` or `owned_by` edge. `false` — entries that have at least one such edge. Omit for all. |
| `project` | no | Override session project |
| `after` | no | Cursor for pagination (entry ID from `next_cursor`) |
| `limit` | no | Max items per page |

### `search_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `query` | yes | Search text |
| `namespace` | no | |
| `kind` | no | Filter results by kind |
| `limit` | no | Max results (default 10) |
| `project` | no | Override session project |
| `min_score` | no | Minimum similarity score 0.0–1.0. Only applied when embeddings are configured |
| `anchor_kind` | no | Resource kind for proximity boost: `task`, `agent`, `knowledge` |
| `anchor_id` | no | Resource ID for proximity boost. Linked entries score +0.2 |
| `task_id` | no | Task ID for task-subgraph proximity boost (BFS depth 3, +0.2) |

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

### `rename_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | Current path |
| `new_path` | yes | New path |
| `namespace` | no | |

### `change_knowledge_kind`

Change the kind of an existing knowledge entry. Does not run on `write_knowledge`
updates — use this tool explicitly. Bumps version when the kind actually changes.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `kind` | yes | Target kind (see `list_knowledge_types`) |
| `namespace` | no | |
| `version` | no | Expected version for optimistic concurrency |

### `tag_knowledge` / `untag_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | yes | |
| `tag` | yes | |
| `namespace` | no | |

### `import_knowledge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `source_project` | yes | Project to import from |
| `path` | yes | Entry path in source project |
| `source_namespace` | no | Namespace in source project |

### `assemble_context`

Traverse the edge graph from a root resource and assemble a rich structured
context object: the root node's content, linked tasks with acceptance criteria
and status, linked knowledge entries (truncated to budget), and the raw edge
list. Useful for giving an agent the full picture of a task or knowledge cluster
without manual graph traversal.

| Parameter | Required | Description |
|-----------|----------|-------------|
| `kind` | yes | Root resource kind: `task`, `knowledge`, `agent` |
| `id` | yes | Root resource ID |
| `max_tokens` | no | Character budget for all content blocks combined (default 4000) |

---

## Graph

Typed directed edges between resources. Use the graph to model relationships
between tasks, knowledge entries, and agents: what a task produced, which
knowledge entry supersedes another, which task spawned which agent.

**Relationship types:** `derived_from`, `produces`, `supersedes`, `merged_from`,
`summarizes`, `implements`, `spawns`, `related_to`, `depends_on`, `invalidates`,
`confirms`, `supported_by`, `contradicted_by`, `owned_by`, `reviewed_by`.

**Resource kinds** (source/target): `task`, `knowledge`, `agent`. (`message` is
not allowed as an edge endpoint.)

Edges can have a `valid_until` expiry (set internally for TTL-based edges) and
are soft-deleted. Historical state is queryable via `as_of`.

### Auto-Created Edges

These operations automatically create edges:

| Operation | Edge Created |
|-----------|--------------|
| `write_knowledge(task_id=...)` | Task →[produces]→ Knowledge |
| `split_task` | Parent →[spawns]→ Children |
| `delegate_task` | Parent →[spawns]→ Subtask |
| `replace_task` | Replacement →[supersedes]→ Original |
| `merge_tasks` | Merged ←[merged_from]— Sources |
| `add_dependency` | Task →[depends_on]→ Dependency |
| `post_task(parent_id=...)` | Parent →[spawns]→ Child |
| `post_task(depends_on=[...])` | Task →[depends_on]→ Each dependency |

| Tool | Session | Description |
|------|---------|-------------|
| `add_edge` | yes | Create a typed directed edge between two resources. |
| `remove_edge` | yes | Soft-delete an edge by ID. |
| `query_relations` | yes | Traverse the graph from an anchor resource and return an `EntityNeighborhood` with inlined peer data. |

### `add_edge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `from_kind` | yes | Source resource kind: `task`, `knowledge`, `agent` |
| `from_id` | yes | Source resource ID |
| `to_kind` | yes | Target resource kind: `task`, `knowledge`, `agent` |
| `to_id` | yes | Target resource ID |
| `rel_type` | yes | Relationship type (see above). Aliases accepted: `blocks`→`depends_on`, `creates`→`produces`, `fulfills`→`implements`, `child_of`→`spawns`, `based_on`→`derived_from` |
| `if_not_exists` | no | `true` (default) — silently skip if the edge already exists. `false` — error on duplicate. |

### `remove_edge`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `edge_id` | yes | Edge UUID to soft-delete |

### `query_relations`

Traverse the graph from an anchor resource and return an `EntityNeighborhood`
with inlined peer data (no separate fetches needed).

| Parameter | Required | Description |
|-----------|----------|-------------|
| `anchor_kind` | yes | `task`, `knowledge`, `agent`, `message` |
| `anchor_id` | yes | UUID for task/agent/message; path for knowledge |
| `rel_types` | no | Filter relation types. Empty or omit = all. Aliases accepted. |
| `direction` | no | `outgoing`, `incoming`, `both` (default) |
| `max_depth` | no | Hop limit (default 1, max recommended 5) |
| `as_of` | no | ISO8601 timestamp — see graph state at a past point in time |

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
| `list_namespaces` | yes | List all registered namespaces for the project. |
| `poll_updates` | yes | Poll for recent domain events since a timestamp. |

### `get_project`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `include_summary` | no | `true` to add agent count, tasks by status, recent completions |

### `update_project`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `description` | no | New project description |
| `version` | no | Expected version for optimistic concurrency |

### `set_project_metadata`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `key` | yes | |
| `value` | yes | |

### `list_namespaces`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `project` | no | Override session project |

### `poll_updates`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `since` | no | ISO 8601 timestamp. Returns events after this time |
| `limit` | no | Max events to return (default 50) |
| `project` | no | Override session project |

---

## License

MIT
