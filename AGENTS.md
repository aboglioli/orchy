# orchy

Multi-agent coordination server. Orchy is the shared infrastructure that
allows multiple AI agents (Claude Code, Codex, Gemini, Cursor, etc.) to work
together on complex goals — like a company operating system for agents.

## What Orchy Does

Orchy exposes **65** MCP tools over Streamable HTTP. Agents connect, register,
and use these tools to coordinate. Orchy enforces the rules; agents bring
the intelligence.

Think of orchy as the operating system for a company made of agents. Every
company needs three things to function: people need to **talk** to each other,
there needs to be **work** to do with clear ownership, and the organization
needs to **remember** what it has learned. Orchy provides all three.

### Communication (Slack for agents)

How agents coordinate in real time.

- **Direct messages** — send to a specific agent by ID
- **Role broadcasts** — send to all agents with a role (`role:reviewer`)
- **Project broadcasts** — send to everyone except yourself
- **Threading** — reply to messages, walk full conversation threads
- **Delivery tracking** — pending, delivered, read status
- **System notifications** — task watchers, review results, and dependency
  failures are delivered as messages to your mailbox automatically

### Work (JIRA/Trello for agents)

How agents organize, claim, and complete work.

- **Tasks** — hierarchical, with dependencies, priorities, tags, and a full
  state machine (pending -> claimed -> in_progress -> completed/failed)
- **Hierarchy** — split tasks into subtasks, delegate without blocking parent,
  merge related tasks. Parent auto-completes when all children finish.
- **Dependencies** — tasks block until dependencies complete. Cascading failure
  notifications when a dependency fails.
- **Reviews** — request approval from a role or agent before proceeding.
  Approve/reject with comments, requester gets notified.
- **Watchers** — subscribe to task status changes, get notified via mailbox.
- **Specs and planning** — use documents for spec-driven development. Write the
  spec first, get it reviewed, then create implementation tasks from it.
- **Resource locks** — prevent two agents from editing the same file or area.
  TTL-based, auto-released on disconnect.

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

## Architecture

Rust. DDD + Hexagonal. Domain layer has zero external dependencies. Store
traits defined in domain, implemented by infrastructure crates.

```
crates/
├── orchy-events/          # reusable event sourcing library (no domain deps)
│   └── src/
│       ├── event.rs       # Event, EventId, RestoreEvent
│       ├── topic.rs       # Topic (dot-separated, validated)
│       ├── namespace.rs   # EventNamespace (domain scope)
│       ├── organization.rs # Organization (tenant scope)
│       ├── payload.rs     # Payload with ContentType (JSON, text, binary)
│       ├── metadata.rs    # Key-value metadata
│       ├── collector.rs   # EventCollector for aggregates
│       ├── serialization.rs # SerializedEvent for DB persistence
│       └── io/            # Acker, Message<A>, Handler, Reader, Writer traits
│
├── orchy-core/            # domain types, traits, services
│   └── src/
│       ├── agent/         # Agent aggregate + AgentStore trait + events
│       ├── task/          # Task + TaskWatcher + ReviewRequest + state machine
│       ├── message/       # Message threading + delivery tracking
│       ├── knowledge/     # Unified knowledge: notes, decisions, skills, context, docs
│       ├── project/       # Project metadata + notes
│       ├── resource_lock/ # TTL-based distributed locking
│       ├── project_link/  # Cross-project resource sharing
│       ├── namespace.rs   # Namespace + ProjectId value objects
│       ├── note.rs        # Note value object
│       ├── error.rs       # Domain error types
│       └── embeddings/    # EmbeddingsProvider trait
│
├── orchy-store-memory/    # in-memory HashMap backend (dev/test)
├── orchy-store-sqlite/    # SQLite + sea-query backend (single-node)
├── orchy-store-pg/        # PostgreSQL + pgvector backend (production)
│
└── orchy-server/          # MCP server binary
    └── src/
        ├── main.rs        # HTTP server + MCP routing
        ├── container.rs   # DI container wiring all services
        ├── config.rs      # config.toml structure
        ├── store.rs       # StoreBackend enum delegation
        ├── bootstrap.rs   # Dynamic bootstrap prompt generation
        ├── heartbeat.rs   # Agent timeout monitor
        └── mcp/
            ├── handler.rs # Session state + ServerHandler + INSTRUCTIONS
            ├── params.rs  # MCP tool parameter structs
            └── tools.rs   # MCP tool implementations
```

### Layer Rules

| Layer | Can Import | Cannot Import |
|-------|-----------|---------------|
| orchy-events | stdlib, serde, uuid, chrono | orchy-core, stores, server |
| orchy-core | stdlib, orchy-events | stores, server |
| orchy-store-* | stdlib, orchy-core, orchy-events | server, other stores |
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
pub trait TaskStore: Send + Sync {
    fn save(&self, task: &mut Task) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &TaskId) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn list(&self, filter: TaskFilter) -> impl Future<Output = Result<Vec<Task>>> + Send;
}
```

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

**Namespace hierarchy** — `/` (root), `/backend`, `/backend/auth`. Reads without
namespace see everything. Writes default to agent's current namespace.

**Optimistic concurrency** — Knowledge entries use `Version` field.

**Resource locking** — TTL-based. Released on disconnect. Use for files, not data.

**Session continuity** — `write_knowledge(kind: "context")` before disconnect, `list_knowledge(kind: "context")` on
startup. Falls back to most recent snapshot in namespace if no own context exists.

### Agent Disconnect Cleanup

When an agent disconnects (or times out via heartbeat monitor):
1. Claimed/in-progress tasks -> released back to Pending
2. Resource locks held by agent -> released
3. Task watchers for agent -> removed
4. Pending reviews assigned to agent -> unassigned

## Agent Lifecycle

**Startup:**
1. `register_agent(project, description)` — roles auto-assigned from task demand
2. `get_project` — metadata; use `include_summary` for task/agent overview
3. `list_knowledge(kind: "skill")` — load conventions
4. `list_knowledge(kind: "context")` — find handoff notes from previous sessions
5. `search_knowledge` — check existing decisions and discoveries
6. `check_mailbox` — inbound; `check_sent_messages` for outbound
7. `get_next_task` — `claim: true` (default) to claim; `claim: false` to peek

**Working:**
- `heartbeat` every ~30s
- `poll_updates` + `check_mailbox` for reactivity
- `watch_task` to track dependencies
- `lock_resource` before editing shared files
- `write_knowledge` for decisions, discoveries, patterns

**Completing:**
- `complete_task` with actionable summary (never just "done")
- `write_knowledge` for each key decision or discovery

**Disconnecting:**
- `write_knowledge(kind: "context", path: "handoff")` with: task ID, progress, blockers, decisions
- `disconnect` — tasks released to pending, locks freed, watchers removed

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

**Paths** identify the topic: `auth-algorithm`, `api-design`, `error-handling`.
Use hierarchy for sub-topics: `auth/jwt-strategy`. Don't repeat the kind in
the path — the kind already categorizes. Scoped by `(project, namespace, path)`.

**Skills** (kind=skill) inherit through namespace hierarchy — child namespaces
override parent skills with the same path.

A new agent joining the project should:
1. `list_knowledge(kind: "skill")` to understand conventions
2. `search_knowledge` to find decisions and discoveries
3. `list_knowledge(kind: "context")` for the latest handoff note
4. Check task notes for progress on specific work

## Maintenance Patterns

A "janitor" agent can compact and reorganize:

- **Compact knowledge** — list related entries, merge into one, delete old ones
- **Extract skills** — find recurring patterns in knowledge, create kind=skill entries
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
- TaskService uses 2 generics: `TS: TaskStore` + `S: AgentStore + WatcherStore + MessageStore + ReviewStore`.
- poll_updates queries the events table, not task projections.
- All tools require a registered session except `register_agent`,
  `list_agents` (when `project` is passed), `list_knowledge_types`, `mark_read`,
  and `list_conversation`.

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

# [skills]
# dir = "skills"

# [embeddings]
# provider = "openai"
# [embeddings.openai]
# url = "https://api.openai.com/v1/embeddings"
# model = "text-embedding-3-small"
# dimensions = 1536
```

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
cargo test -p orchy-core           # domain tests
cargo test -p orchy-events         # event library tests
cargo test -p orchy-store-memory   # in-memory store tests
cargo test -p orchy-store-sqlite   # SQLite tests (in-memory DB)
cargo test -p orchy-store-pg       # PG tests (needs running postgres)
```

For PostgreSQL: `podman compose up -d` (uses `compose.yml`).
