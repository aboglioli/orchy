# orchy — Multi-Agent Coordination MCP Server

**Date:** 2026-04-11
**Status:** Draft
**Approach:** C — Enforced State Machine (dumb but strict)

## Vision

A Rust MCP server daemon that provides shared infrastructure for multiple AI agents
(Claude Code, Gemini CLI, OpenCode, Cursor, Cline, Zed, Antigravity, etc.) to
collaborate on the same goal. orchy is not an orchestrator — it's the coordination
layer. Agents bring the intelligence; orchy enforces the rules of the game.

**What orchy is:** Infrastructure — a task board, shared memory, messaging bus, and
session store exposed as MCP tools over HTTP/SSE. A "service mesh for AI agents."

**What orchy is not:** An agent, an LLM wrapper, a gateway, or an orchestrator with
its own decision-making. No LLM calls happen inside the daemon.

## Project Structure

Rust workspace monorepo. Flat `crates/` layout following the matklad/Nushell/Zed
pattern. Binary is a crate, workspace root is virtual manifest only.

```
orchy/
├── Cargo.toml                  # workspace virtual manifest
├── config.toml                 # daemon configuration
├── compose.yaml                # Postgres + pgvector for development
├── justfile                    # task runner (fmt, test, serve, db-up, etc.)
├── crates/
│   ├── orchy-core/             # domain types, value objects, traits, services
│   ├── orchy-store-memory/     # in-memory storage backend
│   ├── orchy-store-sqlite/     # SQLite + sqlite-vec + FTS5
│   ├── orchy-store-pg/         # Postgres + pgvector
│   ├── orchy-server/           # HTTP/SSE MCP daemon (axum) — the binary
│   └── orchy-cli/              # future CLI client (placeholder)
```

### Crate responsibilities

- **orchy-core:** Domain types (value objects, entities, enums), trait definitions
  (`TaskStore`, `MemoryStore`, `AgentStore`, `MessageStore`, `ContextStore`,
  `EmbeddingsProvider`), service layer (`TaskService`, `MemoryService`, etc.),
  enum dispatch wrappers (`Store`, `EmbeddingsBackend`), search merging (RRF).
  No I/O — only abstractions and business logic.

- **orchy-store-memory:** `MemoryBackend` — HashMap + RwLock implementation of all
  store traits. Substring/cosine search. For tests and prototyping.

- **orchy-store-sqlite:** `SqliteBackend` — rusqlite + sqlite-vec + FTS5. Raw SQL
  queries, no ORM. Hybrid search with FTS5 BM25 + sqlite-vec cosine.

- **orchy-store-pg:** `PgBackend` — sqlx + pgvector. Raw SQL queries, no ORM.
  Hybrid search with tsvector + pgvector HNSW cosine.

- **orchy-server:** Entry point binary. Parses `config.toml`, builds `Container`,
  starts axum HTTP/SSE server and heartbeat monitor. Thin MCP handlers delegate
  to services.

- **orchy-cli:** Future CLI crate. Calls the daemon over HTTP. Placeholder for now.

## Layered Architecture

```
MCP Handlers (thin — parse JSON-RPC, call service, format response)
    ↓
Services (business logic, invariant enforcement, orchestrates store + embeddings)
    ↓
Store Traits (pure data access — raw SQL, no business rules)
```

No business logic in handlers. No business logic in store implementations. Services
own all invariant enforcement and coordination between store and embeddings.

## Domain Model

### Value Objects

All validated at construction, immutable.

**Namespace** — Hierarchical slash-separated path. Same rules as eventure:
- Not empty
- Slash-separated parts: `project/architecture`, `backend/auth`
- Each part: ASCII alphanumeric + hyphen + underscore only
- No empty parts, no leading/trailing slashes

**AgentId** — UUID wrapper, auto-generated on registration.

**TaskId, MessageId, SnapshotId** — UUID wrappers, auto-generated.

**Priority** — Enum: Low, Normal, High, Critical.

**TaskStatus** — Enum: Pending, Blocked, Claimed, InProgress, Completed, Failed.
Valid transitions enforced in the value object.

**AgentStatus** — Enum: Online, Busy, Idle, Disconnected.

**MessageTarget** — Enum: Agent(AgentId), Role(String), Broadcast.

**Version** — u64 wrapper for optimistic concurrency on memory writes.

### Entities

**Agent:**
```
Agent {
    id: AgentId
    namespace: Option<Namespace>
    roles: Vec<String>
    description: String
    status: AgentStatus
    last_heartbeat: DateTime
    connected_at: DateTime
    metadata: HashMap<String, String>
}
```

**Task:**
```
Task {
    id: TaskId
    namespace: Namespace
    title: String
    description: String
    status: TaskStatus
    priority: Priority
    assigned_roles: Vec<String>
    claimed_by: Option<AgentId>
    claimed_at: Option<DateTime>
    depends_on: Vec<TaskId>
    result_summary: Option<String>
    created_by: Option<AgentId>
    created_at: DateTime
    updated_at: DateTime
}
```

**MemoryEntry:**
```
MemoryEntry {
    namespace: Namespace
    key: String
    value: String
    version: u64
    embedding: Option<Vec<f32>>
    embedding_model: Option<String>
    embedding_dimensions: Option<u32>
    written_by: Option<AgentId>
    created_at: DateTime
    updated_at: DateTime
}
```

**Message:**
```
Message {
    id: MessageId
    namespace: Option<Namespace>
    from: AgentId
    to: MessageTarget
    body: String
    status: MessageStatus  // Pending, Delivered, Read
    created_at: DateTime
}
```

**ContextSnapshot:**
```
ContextSnapshot {
    id: SnapshotId
    agent_id: AgentId
    namespace: Option<Namespace>
    summary: String
    embedding: Option<Vec<f32>>
    embedding_model: Option<String>
    embedding_dimensions: Option<u32>
    metadata: HashMap<String, String>
    created_at: DateTime
}
```

## Enforced Invariants

These are enforced in the service layer, not by the agents:

**Tasks:**
- Cannot claim a task unless status is Pending
- Cannot claim a task if any `depends_on` task is not Completed
- Cannot claim a task already claimed by another agent
- Completing a task auto-transitions dependents from Blocked to Pending
- Heartbeat timeout releases claimed tasks back to Pending
- `get_next_task` claims atomically (find + claim in one operation)

**Memory:**
- `write_memory` with `version` param: rejects if stored version doesn't match
  (optimistic concurrency). Without `version`: unconditional overwrite.

**Messages:**
- Delivery to `role:X` fans out to all agents with that role
- Broadcast fans out to all connected agents

**Agents:**
- Heartbeat timeout (configurable) → status set to Disconnected, claimed tasks
  released

## Store Traits

Defined in `orchy-core`. Native `async fn in trait` (stable since Rust 1.75).
No `dyn` — resolved via enum dispatch at startup.

```rust
trait TaskStore {
    async fn create(&self, task: CreateTask) -> Result<Task>;
    async fn get(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>>;
    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task>;
    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task>;
    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task>;
    async fn release(&self, id: &TaskId) -> Result<Task>;
}

trait MemoryStore {
    async fn write(&self, entry: WriteMemory) -> Result<MemoryEntry>;
    async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>>;
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>>;
    async fn search(&self, query: &str, embedding: Option<&[f32]>,
                    namespace: Option<&Namespace>, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()>;
}

trait AgentStore {
    async fn register(&self, registration: RegisterAgent) -> Result<Agent>;
    async fn get(&self, id: &AgentId) -> Result<Option<Agent>>;
    async fn list(&self) -> Result<Vec<Agent>>;
    async fn heartbeat(&self, id: &AgentId) -> Result<()>;
    async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()>;
    async fn disconnect(&self, id: &AgentId) -> Result<()>;
}

trait MessageStore {
    async fn send(&self, message: CreateMessage) -> Result<Message>;
    async fn check(&self, agent: &AgentId) -> Result<Vec<Message>>;
    async fn mark_read(&self, ids: &[MessageId]) -> Result<()>;
}

trait ContextStore {
    async fn save(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot>;
    async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>>;
    async fn list(&self, agent: Option<&AgentId>,
                  namespace: Option<&Namespace>) -> Result<Vec<ContextSnapshot>>;
    async fn search(&self, query: &str, embedding: Option<&[f32]>,
                    namespace: Option<&Namespace>, agent_id: Option<&AgentId>,
                    limit: usize) -> Result<Vec<ContextSnapshot>>;
}
```

Note: `search` methods receive pre-computed embeddings from the service layer.
The store doesn't call the embeddings provider — it only matches vectors.

## Enum Dispatch

Backends are a closed set. No `dyn`, no generics propagating. Resolved once at
startup in `Container`.

```rust
enum Store {
    Memory(MemoryBackend),
    Sqlite(SqliteBackend),
    Postgres(PgBackend),
}

enum EmbeddingsBackend {
    OpenAi(OpenAiEmbeddingsProvider),
    // future: Cohere, HuggingFace, etc.
}
```

Each enum delegates trait methods to the inner implementation. A macro generates
the match-arm boilerplate.

## Services

Defined in `orchy-core`. Business logic and invariant enforcement.

```rust
struct TaskService { store: Store }
struct MemoryService { store: Store, embeddings: Option<EmbeddingsBackend> }
struct AgentService { store: Store }
struct MessageService { store: Store }
struct ContextService { store: Store, embeddings: Option<EmbeddingsBackend> }
```

Service methods:
- `TaskService::claim` — validates dependencies complete, agent exists, task
  status is Pending, then delegates to store.
- `TaskService::complete` — marks task done, queries dependents, transitions
  Blocked → Pending for any unblocked tasks.
- `MemoryService::write` — generates embedding if provider configured, then
  writes to store with vector.
- `MemoryService::search` — generates query embedding via `EmbeddingsBackend`
  (if configured), then calls `store.search()` passing both the raw text query
  and the embedding vector. The store runs keyword and vector searches. The
  service merges results via RRF. If embeddings not configured, only keyword
  results are returned.
- `ContextService::search` — same pattern as memory search.

## Embeddings

### Trait

```rust
trait EmbeddingsProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn model(&self) -> &str;
    fn dimensions(&self) -> u32;
}
```

### Implementation

Single implementation for v1: `OpenAiEmbeddingsProvider` — HTTP client calling
any OpenAI-compatible `/v1/embeddings` endpoint. Works with Ollama, OpenAI,
LiteLLM, vLLM.

### Model migration

Each row stores `embedding_model` and `embedding_dimensions` alongside the vector.
On model change:
1. New writes use the new model
2. Search filters by `WHERE embedding_model = $current_model`
3. Old vectors are stale but not deleted — FTS/keyword still works for those entries
4. Future CLI command `orchy reindex` re-embeds all entries in bulk

### When not configured

Services skip vector generation on write. Search falls back to keyword/FTS only.
No errors, just degraded semantic ranking.

## Search Strategy

Three tiers depending on backend and embeddings config:

| Backend   | Embeddings | Strategy                                           |
|-----------|------------|----------------------------------------------------|
| In-memory | No         | Substring match on value/summary                   |
| In-memory | Yes        | Cosine similarity on stored vectors                |
| SQLite    | No         | FTS5 BM25 ranking                                  |
| SQLite    | Yes        | Hybrid: FTS5 BM25 + sqlite-vec cosine, merged RRF  |
| Postgres  | No         | tsvector + ts_rank                                 |
| Postgres  | Yes        | Hybrid: tsvector ranking + pgvector HNSW cosine, RRF|

**Reciprocal Rank Fusion (RRF):** Runs keyword and vector searches independently.
Scores each result as `1 / (k + rank)` where `k = 60`. Sums scores across both
result sets. Returns top-N by combined score. Lives in the service layer.

## MCP Tools

20 tools across 5 domains. All exposed via HTTP/SSE Streamable MCP.

### Agent Registry (3 tools)

| Tool             | Params                                         | Returns              |
|------------------|------------------------------------------------|----------------------|
| `register_agent` | `roles: [String]`, `description: String`, `namespace?: String` | `Agent` |
| `list_agents`    | `namespace?: String`                           | `[Agent]`            |
| `heartbeat`      | —                                              | `{ ok }`             |

Agent ID auto-assigned on `register_agent`, bound to the MCP session.

### Task Board (6 tools)

| Tool            | Params                                                                                      | Returns       |
|-----------------|--------------------------------------------------------------------------------------------|---------------|
| `post_task`     | `namespace: String`, `title: String`, `description: String`, `priority?: String`, `assigned_roles?: [String]`, `depends_on?: [TaskId]` | `Task` |
| `get_next_task` | `namespace?: String`, `role?: String`                                                      | `Task \| null` |
| `list_tasks`    | `namespace?: String`, `status?: String`                                                    | `[Task]`      |
| `claim_task`    | `task_id: TaskId`                                                                          | `Task`        |
| `complete_task` | `task_id: TaskId`, `summary?: String`                                                      | `Task`        |
| `fail_task`     | `task_id: TaskId`, `reason?: String`                                                       | `Task`        |

`get_next_task` is a convenience that finds the highest priority Pending task
matching the agent's roles and claims it atomically.

### Shared Memory (5 tools)

| Tool             | Params                                                             | Returns             |
|------------------|--------------------------------------------------------------------|---------------------|
| `write_memory`   | `namespace: String`, `key: String`, `value: String`, `version?: u64` | `MemoryEntry`     |
| `read_memory`    | `namespace: String`, `key: String`                                 | `MemoryEntry \| null` |
| `list_memory`    | `namespace?: String`                                               | `[MemoryEntry]`    |
| `search_memory`  | `query: String`, `namespace?: String`, `limit?: u32`               | `[MemoryEntry]`    |
| `delete_memory`  | `namespace: String`, `key: String`                                 | `{ ok }`           |

`write_memory` with `version` enables optimistic concurrency — rejects if stored
version doesn't match.

### Messaging (3 tools)

| Tool            | Params                                               | Returns      |
|-----------------|------------------------------------------------------|--------------|
| `send_message`  | `to: String`, `body: String`, `namespace?: String`   | `Message`    |
| `check_mailbox` | `namespace?: String`                                 | `[Message]`  |
| `mark_read`     | `message_ids: [MessageId]`                           | `{ ok }`     |

`to` accepts: agent ID, `role:<name>` for role-targeted, or `broadcast`.

### Context / Sessions (4 tools)

| Tool              | Params                                                                     | Returns                |
|-------------------|----------------------------------------------------------------------------|------------------------|
| `save_context`    | `summary: String`, `namespace?: String`, `metadata?: Map`                  | `ContextSnapshot`      |
| `load_context`    | `agent_id?: AgentId`                                                       | `ContextSnapshot \| null` |
| `list_contexts`   | `agent_id?: AgentId`, `namespace?: String`                                 | `[ContextSnapshot]`    |
| `search_contexts` | `query: String`, `namespace?: String`, `agent_id?: AgentId`, `limit?: u32` | `[ContextSnapshot]`    |

`load_context` without params loads the calling agent's latest snapshot.
`search_contexts` uses the same hybrid search as memory.

## Database Schema

### SQLite

```sql
CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    namespace TEXT,
    roles TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_heartbeat TEXT NOT NULL,
    connected_at TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles TEXT NOT NULL DEFAULT '[]',
    claimed_by TEXT REFERENCES agents(id),
    claimed_at TEXT,
    depends_on TEXT NOT NULL DEFAULT '[]',
    result_summary TEXT,
    created_by TEXT REFERENCES agents(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE memory (
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    written_by TEXT REFERENCES agents(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (namespace, key)
);

CREATE VIRTUAL TABLE memory_fts USING fts5(
    namespace, key, value,
    content='memory',
    content_rowid='rowid'
);

CREATE VIRTUAL TABLE memory_vec USING vec0(
    rowid INTEGER PRIMARY KEY,
    embedding FLOAT[768]  -- dimension from config, set at table creation
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    namespace TEXT,
    from_agent TEXT NOT NULL REFERENCES agents(id),
    to_target TEXT NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL
);

CREATE TABLE contexts (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id),
    namespace TEXT,
    summary TEXT NOT NULL,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE contexts_fts USING fts5(
    namespace, summary,
    content='contexts',
    content_rowid='rowid'
);

CREATE VIRTUAL TABLE contexts_vec USING vec0(
    rowid INTEGER PRIMARY KEY,
    embedding FLOAT[768]
);
```

### Postgres

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE agents (
    id UUID PRIMARY KEY,
    namespace TEXT,
    roles JSONB NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_heartbeat TIMESTAMPTZ NOT NULL,
    connected_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE tasks (
    id UUID PRIMARY KEY,
    namespace TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles JSONB NOT NULL DEFAULT '[]',
    claimed_by UUID REFERENCES agents(id),
    claimed_at TIMESTAMPTZ,
    depends_on JSONB NOT NULL DEFAULT '[]',
    result_summary TEXT,
    created_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE memory (
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    embedding VECTOR,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    written_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (namespace, key)
);

CREATE INDEX memory_fts_idx ON memory USING gin(to_tsvector('english', value));

CREATE TABLE messages (
    id UUID PRIMARY KEY,
    namespace TEXT,
    from_agent UUID NOT NULL REFERENCES agents(id),
    to_target TEXT NOT NULL,
    body TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE contexts (
    id UUID PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(id),
    namespace TEXT,
    summary TEXT NOT NULL,
    embedding VECTOR,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX contexts_fts_idx ON contexts USING gin(to_tsvector('english', summary));
```

HNSW vector indexes created dynamically at startup when embeddings are configured:
```sql
CREATE INDEX IF NOT EXISTS memory_vec_idx
    ON memory USING hnsw (embedding vector_cosine_ops);
CREATE INDEX IF NOT EXISTS contexts_vec_idx
    ON contexts USING hnsw (embedding vector_cosine_ops);
```

## MCP Server and Transport

HTTP/SSE Streamable MCP (2025 spec):
- `POST /mcp` — JSON-RPC requests (tool calls, initialization)
- `GET /mcp` — SSE stream (server-to-client notifications)

### Session management

On `initialize` handshake, server creates a session UUID, returns it in
`Mcp-Session-Id` header. All subsequent requests include this header. Session
tracks agent identity after `register_agent`.

### Server internal structure

```
orchy-server/src/
    main.rs          -- parse config, build container, start server
    config.rs        -- Config struct, TOML deserialization
    container.rs     -- Container: assembles services from config
    mcp/
        router.rs    -- axum routes: POST /mcp, GET /mcp
        session.rs   -- session tracking, Mcp-Session-Id management
        handler.rs   -- JSON-RPC dispatch: method name → service call
        tools.rs     -- tool definitions (name, description, input schema)
    heartbeat.rs     -- background task: agent liveness, stale task release
```

### Heartbeat monitor

Background tokio task running every `heartbeat_timeout_secs / 2`. Queries
`AgentService` for agents past timeout, calls disconnect, releases their
claimed tasks.

## Configuration

```toml
[server]
host = "127.0.0.1"
port = 3100
heartbeat_timeout_secs = 60

[store]
backend = "sqlite"  # "memory" | "sqlite" | "postgres"

[store.sqlite]
path = "orchy.db"

[store.postgres]
url = "postgres://localhost:5432/orchy"

[embeddings]
provider = "openai"  # only option for now

[embeddings.openai]
url = "http://localhost:11434/v1/embeddings"
model = "nomic-embed-text"
dimensions = 768
```

## Container (Startup Assembly)

```rust
struct Container {
    task_service: TaskService,
    memory_service: MemoryService,
    agent_service: AgentService,
    message_service: MessageService,
    context_service: ContextService,
    config: Config,
}
```

`Container::new(config)` reads the config, constructs the `Store` enum variant,
optionally the `EmbeddingsBackend`, wires services, and returns the assembled
container. Passed as `Arc<Container>` to all handlers.

## Future Considerations

- **CLI crate:** `orchy-cli` calls the daemon over HTTP for manual task/memory
  management, bulk operations like `orchy reindex`.
- **Supervisor layer:** PTY wrapper that spawns agents as child processes and
  injects messages into their stdin. Builds on top of the existing MCP server —
  the supervisor is just another client.
- **Dashboard:** Web UI for managing tasks, memory, agents. Reads from the same
  HTTP endpoint.
- **Event subscriptions:** SSE notifications when tasks complete, messages arrive,
  memory changes — agents subscribe instead of polling.
- **File lock tracking:** Agents declare which files they're editing, others are
  warned of conflicts.
- **Vector reindexing:** CLI command to re-embed all entries on model change.
