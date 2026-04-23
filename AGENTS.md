# orchy

Multi-agent coordination server. Orchy is the shared infrastructure that
allows multiple AI agents (Claude Code, Codex, Gemini, Cursor, etc.) to work
together on complex goals — like a company operating system for agents.

## What Orchy Does

Orchy exposes **62** MCP tools over Streamable HTTP and a stateless CLI
(`orchy`) for agents without MCP support. Agents connect, register, and use
these tools to coordinate. Orchy enforces the rules; agents bring the
intelligence.

Think of orchy as the operating system for a company made of agents. Every
company needs three things to function: people need to **talk** to each other,
there needs to be **work** to do with clear ownership, and the organization
needs to **remember** what it has learned. Orchy provides all three.

### Communication (Slack for agents)

How agents coordinate in real time.

- **Direct messages** — send to a specific agent by alias (`@coder-1`)
- **Role broadcasts** — send to all agents with a role (`role:reviewer`)
- **Namespace broadcasts** — send to all agents in a namespace (`ns:/backend`)
- **Project broadcasts** — send to everyone except yourself (`broadcast`)
- **Threading** — reply to messages, walk full conversation threads
- **Delivery tracking** — pending, delivered, read status
- **System notifications** — dependency failures are delivered as messages to
  your mailbox automatically

### Work (JIRA/Trello for agents)

How agents organize, claim, and complete work.

- **Tasks** — hierarchical, with dependencies, priorities, tags, and a full
  state machine (pending -> claimed -> in_progress -> completed/failed)
- **Hierarchy** — split tasks into subtasks, delegate without blocking parent,
  merge related tasks. Parent auto-completes when all children finish.
- **Dependencies** — tasks block until dependencies complete. Cascading failure
  notifications when a dependency fails.
- **Specs and planning** — use documents for spec-driven development. Write the
  spec first, then create implementation tasks from it.
- **Resource locks** — prevent two agents from editing the same file or area.
  TTL-based, auto-released on disconnect.
- **Task staleness** — tasks become stale after inactivity, claimable by other
  agents. No automatic release on disconnect.

### Knowledge (Notion/Wiki for agents)

How the organization remembers what it has learned.

Agents don't retain state between sessions. Every insight, decision, and
finding must be externalized or it's lost. All knowledge lives in a **unified
module** with typed entries (`kind`). Each entry has a `path` for hierarchical
organization and `tags` for cross-cutting labels.

- **Decisions** (`kind: decision`) — choices made with rationale. "We chose
  RS256 over HS256 for key rotation support."
- **Discoveries** (`kind: discovery`) — things found or learned during work.
  Gotchas, constraints, performance findings.
- **Documents** (`kind: document`) — long-form specs, architecture decisions,
  analysis, post-mortems.
- **Skills** (`kind: skill`) — reusable conventions and instructions that all
  agents must follow. Inherited through namespace hierarchy.
- **Contexts** (`kind: context`) — session handoff snapshots. What you were
  working on, what's left. The next agent loads this to continue your work.
- **Patterns**, **plans**, **configs**, **references**, **notes**, **logs** — see
  `list_knowledge_types` for the full set.
- **Cross-project sharing** — link projects to import knowledge entries. A
  "global" project serves as a shared resource pool across all projects.
- **Semantic search** — `search_knowledge` finds relevant entries by meaning,
  not just exact match. Powered by embeddings when configured.
- **Temporal validity** — knowledge entries have optional `valid_from`/`valid_until`.
  Expired entries excluded from search by default.
- **Promotion** — promote a decision/discovery/pattern into a reusable `skill`.
- **Consolidation** — merge related knowledge entries into one, deleting sources.

## Architecture

Rust. DDD + Hexagonal. Domain layer has zero external dependencies. Store
traits defined in domain, implemented by infrastructure crates.

```
crates/
├── orchy-events/          # reusable event sourcing library (no domain deps)
│   └── src/
│       ├── event.rs       # Event, EventId, RestoreEvent
│       ├── topic.rs       # Topic (dot-separated, validated)
│       ├── namespace.rs   # Namespace (canonical type used by all crates)
│       ├── organization.rs # Organization (tenant scope)
│       ├── payload.rs     # Payload with ContentType (JSON, text, binary)
│       ├── metadata.rs    # Key-value metadata
│       ├── collector.rs   # EventCollector for aggregates
│       ├── serialization.rs # SerializedEvent for DB persistence
│       └── io/            # Acker, Message<A>, Handler, Reader, Writer traits
│
├── orchy-core/            # domain types, traits, store traits
│   └── src/
│       ├── agent/         # Agent aggregate + AgentStore trait + events
│       ├── task/          # Task + state machine
│       ├── message/       # Message threading + delivery tracking
│       ├── knowledge/     # Unified knowledge: notes, decisions, skills, context, docs
│       ├── graph/         # Edge aggregate, GraphStore trait, traversal, neighborhoods
│       │   ├── mod.rs     # Edge, EdgeId, EdgeStore, RelationType, TraversalDirection
│       │   ├── neighborhood.rs  # EntityNeighborhood, PeerEntity, Relation summaries
│       │   ├── relation_options.rs  # RelationOptions, RelationQuery
│       │   └── rules.rs   # Graph validation rules (cycle detection)
│       ├── project/       # Project metadata
│       ├── organization/  # Organization aggregate + events
│       ├── resource_lock/ # TTL-based distributed locking
│       ├── namespace.rs   # ProjectId value object
│       ├── resource_ref.rs # ResourceRef for cross-entity links
│       ├── pagination.rs  # PageParams + Page<T> for paginated queries
│       ├── error.rs       # Domain error types
│       └── embeddings/    # EmbeddingsProvider trait
│
├── orchy-application/     # use cases / application layer
│
├── orchy-store-memory/    # in-memory HashMap backend (dev/test)
├── orchy-store-sqlite/    # SQLite + sea-query backend (single-node)
├── orchy-store-pg/        # PostgreSQL + pgvector backend (production)
│
├── orchy-cli/             # stateless CLI binary (`orchy`) — REST client for non-MCP agents
│
└── orchy-server/          # MCP + REST API server binary
    └── src/
        ├── main.rs        # HTTP server + MCP routing
        ├── container.rs   # DI container wiring all services
        ├── config.rs      # config.toml structure
        ├── store.rs       # StoreBackend enum delegation
        ├── bootstrap.rs   # Dynamic bootstrap prompt generation
        ├── heartbeat.rs   # Agent timeout monitor
        ├── api/           # REST handlers (axum)
        │   ├── graph.rs   # Graph endpoints: add_edge, remove_edge, query_relations, assemble_context
        │   └── ...
        └── mcp/
            ├── handler.rs # Session state + ServerHandler + INSTRUCTIONS
            ├── params.rs  # MCP tool parameter structs
            └── tools/     # MCP tool implementations (one file per domain)
```

### Layer Rules

| Layer | Can Import | Cannot Import |
|-------|-----------|---------------|
| orchy-events | stdlib, serde, uuid, chrono | orchy-core, application, stores, server |
| orchy-core | stdlib, orchy-events | application, stores, server |
| orchy-application | stdlib, orchy-core, orchy-events | stores, server |
| orchy-store-* | stdlib, orchy-core, orchy-events | application, server, other stores |
| orchy-server | everything | — |

## Key Patterns

### Event Sourcing

Every aggregate has an `EventCollector`. Every mutation collects a semantic
event. Every `save()` drains events and persists them via `io::Writer`.

```
Aggregate mutation -> EventCollector.collect() -> save() -> drain_events() -> Writer::write()
```

Events go to an `events` table in the same database as projections. The event
log is append-only. Projections (entity tables) are denormalized views.

Delete operations go through the aggregate: `mark_deleted()` -> `save()` (persists
event) -> `store.delete()` (removes projection).

### Constructor Convention

| Pattern | Purpose | Events? |
|---------|---------|---------|
| `Entity::new(...)` | First-time creation with validation | Yes |
| `Entity::restore(RestoreX { ... })` | Reconstruct from DB, no validation | No |

All `restore()` methods take a single struct with named fields (not positional
params). The `RestoreX` struct has public fields.

### Store Trait Pattern

Domain defines traits. Each store crate implements them. `StoreBackend` enum
in server delegates via macro. All `save()` methods take `&mut` to drain events.

```rust
#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn save(&self, task: &mut Task) -> Result<()>;
    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>>;
}
```

### Application Service Contract

Every use case in `orchy-application` follows:
- Command DTO in, Response DTO out
- No domain aggregates cross the application boundary
- `Arc<dyn Trait>` for store dependencies

### Task State Machine

```
Pending -> Claimed -> InProgress -> Completed
   |         |          |
   v         v          v
Blocked   Failed     Failed
   |         |          |
   v         v          v
Cancelled Cancelled  Cancelled
```

Also: `Claimed -> Blocked`, `InProgress -> Blocked` (for split_task).
`Blocked -> Pending` (unblock). Parent tasks auto-complete when all children finish.

### Other Patterns

**Value objects** — Immutable, validated on creation, never direct-cast.
UUID v7 for all IDs (time-ordered).

**Value Object Pattern** — Value objects are used from `execute()` downward, NOT in command DTOs:

```
API/MCP Handler (String input)
    ↓ parse with value object constructors
Command DTO (String fields) ✅ OK as-is
    ↓ parse to value objects inside execute()
Application Service: execute() ✅ PARSE HERE
    ↓
Store Trait: method(&VO) ✅ REQUIRED
    ↓
Domain Entity
```

Key value objects:
- `Alias` — agent identity: lowercase alphanumeric + hyphens, 2-32 chars
- `KnowledgePath` — knowledge entry path: no slashes at edges, alphanumeric segments
- UUID-based IDs (`AgentId`, `TaskId`, `MessageId`, etc.) — already have FromStr

**Namespace hierarchy** — `/` (root), `/backend`, `/backend/auth`. Reads without
namespace see everything. Writes default to agent's current namespace.

**Optimistic concurrency** — Knowledge entries use `Version` field.

**Resource locking** — TTL-based. Released on disconnect. Use for files, not data.

**Session continuity** — `write_knowledge(kind: "context")` before disconnect.
On startup, `register_agent` returns skills, handoff context, inbox, and
pending tasks in one call.

### Agent Disconnect Cleanup

When an agent stops heartbeating or times out:
1. Agent status becomes `stale` (derived from `last_seen`)
2. Stale tasks become claimable by other agents (first-writer-wins)
3. Resource locks are released

No automatic task release on disconnect. Tasks must be explicitly released
or become stale through inactivity.

## Agent Lifecycle

### MCP agents

**Startup:**
1. `register_agent(alias="coder-1", project, description)` — roles auto-assigned from task demand. Returns full context: agent info, inbox, pending tasks, skills, handoff context, rescue info.
2. `get_next_task` — `claim: true` (default) to claim; `claim: false` to peek
3. `heartbeat` every ~30s (updates `last_seen`)

**Working:**
- `poll_updates` + `check_mailbox` for reactivity
- `lock_resource` before editing shared files
- `write_knowledge` for decisions, discoveries, patterns
- `touch_task` for long-running work (prevents staleness)

**Completing:**
- `complete_task` with actionable summary (never just "done")
- `write_knowledge` for each key decision or discovery

**Renaming:**
- `rename_alias(new_alias)` — change alias, all internal references use UUID so nothing breaks

**Disconnecting:**
- `write_knowledge(kind: "context", path: "handoff")` with: task ID, progress, blockers, decisions
- No `disconnect` tool — agents just stop calling. Tasks become stale, locks released.

### CLI agents (stateless, no MCP)

For agents without MCP support (pi coding agent, Codex CLI, shell scripts). Each call is an independent REST request — no session, no heartbeat.

**Config** (lowest → highest priority):
1. `~/.orchy/config.toml` — global
2. `.orchy.toml` — repo-local (walk up from cwd)
3. Env vars: `ORCHY_URL`, `ORCHY_API_KEY`, `ORCHY_ORG`, `ORCHY_PROJECT`, `ORCHY_NAMESPACE`, `ORCHY_ALIAS`
4. Per-call flags

**Startup:**
```bash
orchy bootstrap --json      # briefing: inbox, tasks, skills, handoff context
orchy agent register --alias coder-1 --description "Backend dev"
```

**Working:**
```bash
orchy task next --json                          # peek or claim next task
orchy task claim <id>
orchy task start <id>
orchy task touch <id>                           # keep-alive for long-running work
orchy knowledge write <path> --kind decision --title "..." --content "..." --task-id <id>
orchy message send --to @architect --body "..."
orchy message send --to role:reviewer --body "..."
orchy message send --to ns:/backend --body "..."
orchy message send --to broadcast --body "..."
orchy lock acquire <name> --ttl 300
orchy event poll --json
```

**Completing:**
```bash
orchy task complete <id> --summary "..."
orchy knowledge write handoff --kind context --title "Handoff" --content "..."
```

`register_agent`, `heartbeat`, `disconnect`, and `session_status` do not exist in the CLI. Use `ORCHY_ALIAS` (or `--alias`) to identify yourself across calls.

## Knowledge Module

All persistent knowledge lives in a single unified module with typed entries.
Use `list_knowledge_types` to discover available kinds.

| Kind | Use for |
|------|---------|
| `note` | general observations and records |
| `decision` | choices made with rationale |
| `discovery` | things found or learned |
| `pattern` | recurring approaches or conventions |
| `context` | session summaries / agent state snapshots |
| `document` | long-form specs, analysis, architecture |
| `config` | configuration or setup information |
| `reference` | external references or links |
| `plan` | strategies, roadmaps, approaches |
| `log` | activity or change log entries |
| `skill` | instructions/conventions agents must follow |
| `overview` | project summaries surfaced in bootstrap prompts |
| `summary` | compact synthesized output: task summaries, agent rollups, state snapshots |
| `report` | richer completion artifact: implementation reports, post-task writeups |

**Paths** identify the topic: `auth-algorithm`, `api-design`, `error-handling`.
Use hierarchy for sub-topics: `auth/jwt-strategy`. Don't repeat the kind in
the path — the kind already categorizes. Scoped by `(project, namespace, path)`.

**Skills** (kind=skill) inherit through namespace hierarchy — child namespaces
override parent skills with the same path.

A new agent joining the project should:
1. `register_agent` returns skills, handoff context, inbox, and pending tasks
2. `search_knowledge` to find decisions and discoveries relevant to the assigned work

## Maintenance Patterns

A "janitor" agent can compact and reorganize:

- **Compact knowledge** — list related entries, merge into one, delete old ones
  via `consolidate_knowledge`
- **Extract skills** — find recurring patterns in knowledge, create kind=skill entries
  via `promote_knowledge`
- **Reorganize tasks** — merge related items, move to correct namespace
- **Lock during compaction** — `lock_resource("compaction")` to prevent conflicts

## Decisions Log

- Memory uses optimistic concurrency (Version field), not locks. ResourceLock
  handles external resource locking with TTL.
- EventLog trait replaced by io::Writer — stores implement Writer directly.
  Events persisted through: aggregate -> drain -> Writer::write.
- sea-query for dynamic SQL in sqlite/pg stores. Recursive CTEs and FTS queries
  remain as raw SQL (not expressible in sea-query's AST).
- Restore structs over positional params for DB reconstruction.
- UUID v7 everywhere for time-ordered identifiers.
- Embeddings provider lives in server crate, core only defines the trait.
- TaskService was removed; all orchestration lives in orchy-application use cases.
- poll_updates queries the events table, not task projections.
- All tools require a registered session except `register_agent`,
  `session_status`, `list_knowledge_types`, and `list_agents` (when `project`
  is passed).
- `Task.parent_id`, `Task.depends_on[]`, and `Knowledge.agent_id` were removed
  as first-class DB columns. Relationships now live in the Edge graph layer:
  `spawns` (parent→child), `depends_on` (task→task), `owned_by` (knowledge→agent).
  Edge creation is automatic for `split_task`, `delegate_task`, `merge_tasks`,
  `add_dependency`, and `write_knowledge` with `task_id`.
- `get_neighbors`, `get_graph`, `list_edges` MCP tools removed — superseded by
  `query_relations` (richer neighborhood traversal with semantic re-ranking).
- **Alias-based identity** replaces UUID-based registration. Agents register
  and reconnect via `(org, project, alias)`. UUID is internal only.
- **Last-writer-wins on registration** — re-registering with same alias resumes
  existing agent. No lockouts.
- **Status derived from `last_seen`** — no stored state machine. Status is
  computed as active/idle/stale from elapsed time since last heartbeat.
- **Task staleness replaces auto-release** — tasks stay claimed but become
  claimable after `stale_after_secs` of inactivity. `touch_task` keeps alive.
- **Messages resolved at read time** — role/ns/broadcast targets stored as raw
  strings, resolved dynamically when reading inbox.
- **No disconnect tool** — agents stop calling. Tasks become stale naturally.
  Resource locks released via heartbeat monitor.
- **Auth-derived agent ownership** — API key resolution returns `ApiKeyPrincipal`
  with org + user_id. Agent registration derives ownership from the authenticated
  principal, not caller-supplied values. Ownership resume rules: none→attach,
  same→resume, different→conflict.
- **User-targeted messages are logical** — `user:<uuid>` targets are stored as
  raw strings and resolved dynamically at mailbox-read time against the agent's
  persisted `user_id`. No fan-out at send time.
- **Claim semantics for logical targets** — role/ns/broadcast/user messages support
  optional claim/unclaim. Claimed logical messages are hidden from sibling default
  inboxes but remain visible in history/thread views.
- **MCP/REST alias parity** — both REST and MCP resolve `@alias` in send_message
  and lock_resource. Alias uniqueness enforced per `(org, project, alias)`.
- **Namespace mark-read via receipts** — namespace-targeted messages create receipts
  on read, enabling consistent mark_read behavior with other logical targets.

## Configuration

```toml
[server]
host = "127.0.0.1"
port = 3100
heartbeat_timeout_secs = 300

[store]
backend = "sqlite"    # "sqlite", "postgres", or "memory"

[store.sqlite]
path = "orchy.db"

# [store.postgres]
# url = "postgres://orchy:orchy@localhost:5432/orchy"

# [auth]
# jwt_duration_hours = 24
# cookie_secure = false
# bcrypt_cost = 10
# keys_dir = "keys"

# [skills]
# dir = "skills"

# [embeddings]
# provider = "openai"
# [embeddings.openai]
# url = "https://api.openai.com/v1/embeddings"
# model = "text-embedding-3-small"
# dimensions = 1536
```

## Documentation Policy

- `docs/` is tracked in git and is for durable, human-facing project documentation.
- Agents MUST NOT write to `docs/` or commit into it unless explicitly requested by a human.
- Commit only docs written for humans: architecture notes, operator/user docs, ADRs, status docs, migration notes, and similar long-lived references.
- Never commit `docs/superpowers/**`.
- Never commit agent-only artifacts anywhere under `docs/`: plans for agents, scratch analysis, investigation dumps, validation reports, prompt/session artifacts, or internal execution notes.
- If agent work produces useful insight, rewrite it into a concise human-facing document before committing it under `docs/`.
- When in doubt, do not commit the doc until it is clearly useful to a human reader who is not reconstructing agent context.

## Code Style

- Rust edition 2024, DDD + Hexagonal
- No comments unless essential. No helper/utils files.
- Traits first in each file, then types, constructors, methods, getters
- Return early / guard clauses, no else after return
- `cargo fmt` before committing
- Conventional commits: `type(scope): description`
- No GPG signing, no Co-Authored-By, never push

## Running

```bash
cargo run -p orchy-server          # start MCP server (default: config.toml)
cargo run -p orchy-cli -- --help   # CLI binary (orchy)
cargo test -p orchy-core           # domain tests
cargo test -p orchy-events         # event library tests
cargo test -p orchy-application    # application layer tests
cargo test -p orchy-store-memory   # in-memory store tests
cargo test -p orchy-store-sqlite   # SQLite tests (in-memory DB)
cargo test -p orchy-store-pg       # PG tests (needs running postgres)
```

For PostgreSQL: `podman compose up -d` (uses `compose.yml`).
