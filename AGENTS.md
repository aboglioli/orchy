# orchy — Agent Instructions

Multi-agent coordination server in Rust. Shared infrastructure for AI agents:
task board, shared memory, messaging, skill registry, documents, and project
context — exposed as MCP tools over Streamable HTTP.

## Architecture

DDD + Hexagonal. Domain layer has zero external dependencies. Store traits
defined in domain, implemented by infrastructure crates.

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
│       └── io/            # event IO abstractions
│           ├── mod.rs     # Acker, Message<A>, Handler, Reader, Writer traits
│           └── ackers/    # NoopAcker, OnceAcker
│
├── orchy-core/            # domain types, traits, services
│   └── src/
│       ├── agent/         # Agent aggregate + AgentStore trait + events
│       ├── task/          # Task aggregate + hierarchy + state machine + events
│       ├── message/       # Message threading + delivery tracking + events
│       ├── memory/        # Key-value shared memory + versioning + events
│       ├── skill/         # Project conventions/instructions + events
│       ├── project/       # Project metadata + notes + events
│       ├── document/      # Document storage + versioning + events
│       ├── resource_lock/ # TTL-based resource locking + events
│       ├── project_link/  # Cross-project resource sharing + events
│       ├── namespace.rs   # Namespace + ProjectId value objects
│       ├── note.rs        # Note value object
│       ├── error.rs       # Domain error types
│       ├── embeddings/    # EmbeddingsProvider trait + search algorithm
│       └── infrastructure/ # MockStore (test-only, cfg(test))
│
├── orchy-store-memory/    # in-memory HashMap backend (dev/test)
├── orchy-store-sqlite/    # SQLite backend (single-node)
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
        ├── skill_loader.rs # Load skills from disk on startup
        ├── embeddings.rs  # OpenAI embeddings provider (infra)
        └── mcp/
            ├── handler.rs # Session state + ServerHandler + INSTRUCTIONS
            ├── params.rs  # MCP tool parameter structs
            └── tools.rs   # 70+ MCP tool implementations
```

## Layer Rules

| Layer | Can Import | Cannot Import |
|-------|-----------|---------------|
| orchy-events | stdlib, serde, uuid, chrono | orchy-core, stores, server |
| orchy-core | stdlib, orchy-events | stores, server |
| orchy-store-* | stdlib, orchy-core, orchy-events | server, other stores |
| orchy-server | everything | — |

Domain tests use `infrastructure::MockStore` (inside orchy-core, `cfg(test)`).
Integration tests in store crates use real backends.

## Key Patterns

### Constructor Convention

| Pattern | Purpose | Events? | Example |
|---------|---------|---------|---------|
| `Entity::new(...)` | First-time creation with validation | Yes | `Task::new(...)` returns `Result<Self>` |
| `Entity::restore(RestoreEntity { ... })` | Reconstruct from DB, no validation | No | `Task::restore(r)` |

All `restore()` methods take a single struct with named fields (not positional
params). The `RestoreX` struct has public fields.

### Event Sourcing

Every aggregate has an `EventCollector`. Every mutation collects a semantic
event. Every `save()` drains events and persists them via `io::Writer`.

```
Aggregate mutation → EventCollector.collect() → save() → drain_events() → Writer::write()
```

Events go to an `events` table in the same database as projections. The event
log is append-only. Projections (entity tables) are denormalized views.

Delete operations go through the aggregate: `mark_deleted()` → `save()` (persists
event) → `store.delete()` (removes projection).

55 event topics across 9 aggregates. `heartbeat` and `set_embedding` are
excluded (ephemeral/infra, not domain state).

### State Machine (Tasks)

```
Pending → Claimed → InProgress → Completed
   |         |          |
   v         v          v
Blocked   Failed     Failed
   |         |          |
   v         v          v
Cancelled Cancelled  Cancelled
```

Also: `Claimed → Blocked`, `InProgress → Blocked` (for split_task).
`Blocked → Pending` (unblock). Parent tasks auto-complete when all children finish.

### Store Trait Pattern

Domain defines traits. Each store crate implements them. `StoreBackend` enum
in server delegates via macro.

```rust
pub trait TaskStore: Send + Sync {
    fn save(&self, task: &mut Task) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &TaskId) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn list(&self, filter: TaskFilter) -> impl Future<Output = Result<Vec<Task>>> + Send;
}
```

All `save()` methods take `&mut` to drain events.

### Value Objects

Immutable, validated on creation, never direct-cast. Use `TryFrom<String>` for
validation, `FromStr` for parsing. All ID types use UUID v7 (time-ordered).

### Namespace Hierarchy

Resources live in namespaces: `/` (root), `/backend`, `/backend/auth`.
Omit namespace on reads to see everything. Writes default to the agent's
current namespace. Namespaces are auto-created on first use.

### Optimistic Concurrency

Memory entries use a `Version` field. Writers pass `expected_version` to detect
concurrent modifications. No memory-level locks — use `ResourceLock` for
external resource locking.

### Resource Locking

`ResourceLock` has TTL-based expiry. Any named resource (file, deployment,
refactoring scope) can be locked. Locks are released on agent disconnect
(graceful or timeout). The heartbeat monitor cleans up timed-out agents.

### Agent Disconnect Cleanup

When an agent disconnects (or times out via heartbeat monitor):
1. Claimed/in-progress tasks → released back to Pending
2. Resource locks held by agent → released
3. Task watchers for agent → removed
4. Pending reviews assigned to agent → unassigned

### Project Links

Projects can link to other projects to share resources (skills, memory,
documents). A reserved "global" project can serve as a shared resource pool
by linking other projects to it.

### Bootstrap Prompt

Generated dynamically by `bootstrap.rs`. Combines:
- Static: coordination instructions, namespace rules, task workflow
- Dynamic: project description, notes, skills, connected agents, active tasks

Available via `get_bootstrap_prompt` tool or HTTP GET `/bootstrap/<project>`.

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

## Decisions Log

### Memory locks removed
Memory entries use optimistic concurrency via `version` field instead of locks.
The `ResourceLock` system handles external resource locking with TTL.

### EventLog replaced by io::Writer
The `EventLog` trait was removed. Store backends implement `io::Writer`
directly. Events are persisted through a single path: aggregate → drain →
Writer::write. No inline SQL duplication.

### sea-query for dynamic SQL
SQLite and PostgreSQL stores use sea-query for dynamic WHERE clause building.
Replaces manual `idx`/`params` string interpolation. Recursive CTEs and FTS
queries remain as raw SQL (not expressible in sea-query's AST).
`sea-query-rusqlite` 0.7 has a compatibility issue with sea-query 0.32 —
needs monitoring for a fix.

### Restore structs over positional params
All `restore()` methods take a single `RestoreX` struct with named public
fields instead of 10-18 positional parameters. Prevents column-ordering bugs
when adding fields.

### UUID v7 everywhere
All IDs use `Uuid::now_v7()` for time-ordered identifiers. Enables natural
sorting by creation time.

### Infrastructure in core
Test mocks live in `orchy-core/src/infrastructure/` behind `#[cfg(test)]`.
The domain layer has no external crate dependencies for testing.

### Embeddings provider in server
`OpenAiEmbeddingsProvider` lives in `orchy-server/src/embeddings.rs`, not in
core. Core only defines the `EmbeddingsProvider` trait. Services are generic
over `E: EmbeddingsProvider`.

## Code Style

- Rust edition 2024
- snake_case files, PascalCase types, SCREAMING_SNAKE constants
- No comments unless essential for context
- No helper/utils files — put implementations where they belong
- Interfaces/traits first in each file, then types, constructors, methods, getters
- Return early / guard clauses, no else after return
- `cargo fmt` before committing
- Conventional commits: `type(scope): description`

## Running

```bash
cargo run -p orchy-server          # start MCP server
cargo test -p orchy-core           # domain tests
cargo test -p orchy-events         # event library tests
cargo test -p orchy-store-memory   # in-memory store tests
cargo test -p orchy-store-sqlite   # SQLite tests (in-memory DB)
cargo test -p orchy-store-pg       # PG tests (needs running postgres)
```

For PostgreSQL: `podman compose up -d` (uses `compose.yml`).
