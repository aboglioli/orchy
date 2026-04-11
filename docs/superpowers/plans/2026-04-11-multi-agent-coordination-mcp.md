# Multi-Agent Coordination MCP Server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust MCP server daemon that provides task board, shared memory, messaging, and context management for multiple AI agents to collaborate.

**Architecture:** Enforced state machine — services enforce invariants (task dependencies, locking, heartbeats), storage is pluggable via enum dispatch (in-memory, SQLite, Postgres). MCP transport via rmcp 1.4 Streamable HTTP with axum 0.8. No ORM — raw SQL.

**Tech Stack:** Rust 2024 edition, rmcp 1.4 (MCP SDK), axum 0.8, rusqlite + sqlite-vec + FTS5, sqlx + pgvector, tokio, serde, uuid, chrono, reqwest, toml, tracing

**Spec:** `docs/superpowers/specs/2026-04-11-multi-agent-coordination-mcp-design.md`

---

## File Map

```
orchy/
├── Cargo.toml
├── config.toml
├── compose.yaml                          (exists)
├── justfile                              (exists)
├── crates/
│   ├── orchy-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs                  -- Error enum, Result alias
│   │       ├── value_objects/
│   │       │   ├── mod.rs
│   │       │   ├── namespace.rs          -- Namespace (from eventure pattern)
│   │       │   ├── ids.rs                -- AgentId, TaskId, MessageId, SnapshotId
│   │       │   ├── priority.rs           -- Priority enum
│   │       │   ├── task_status.rs        -- TaskStatus enum with transitions
│   │       │   ├── agent_status.rs       -- AgentStatus enum
│   │       │   ├── message_target.rs     -- MessageTarget enum (Agent|Role|Broadcast)
│   │       │   └── version.rs            -- Version u64 wrapper
│   │       ├── entities/
│   │       │   ├── mod.rs
│   │       │   ├── agent.rs              -- Agent entity
│   │       │   ├── task.rs               -- Task entity + CreateTask, TaskFilter
│   │       │   ├── memory_entry.rs       -- MemoryEntry + WriteMemory, MemoryFilter
│   │       │   ├── message.rs            -- Message + CreateMessage
│   │       │   └── context.rs            -- ContextSnapshot + CreateSnapshot
│   │       ├── store/
│   │       │   ├── mod.rs                -- Store enum dispatch + re-exports
│   │       │   ├── task_store.rs         -- TaskStore trait
│   │       │   ├── memory_store.rs       -- MemoryStore trait
│   │       │   ├── agent_store.rs        -- AgentStore trait
│   │       │   ├── message_store.rs      -- MessageStore trait
│   │       │   └── context_store.rs      -- ContextStore trait
│   │       ├── embeddings/
│   │       │   ├── mod.rs                -- EmbeddingsBackend enum + trait
│   │       │   └── openai.rs             -- OpenAiEmbeddingsProvider
│   │       ├── services/
│   │       │   ├── mod.rs
│   │       │   ├── task_service.rs       -- TaskService (invariant enforcement)
│   │       │   ├── memory_service.rs     -- MemoryService (embeddings + RRF)
│   │       │   ├── agent_service.rs      -- AgentService
│   │       │   ├── message_service.rs    -- MessageService (fan-out)
│   │       │   └── context_service.rs    -- ContextService (embeddings + RRF)
│   │       └── search.rs                 -- RRF merge function
│   ├── orchy-store-memory/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                    -- MemoryBackend struct + all trait impls
│   │       ├── agent.rs                  -- AgentStore impl
│   │       ├── task.rs                   -- TaskStore impl
│   │       ├── memory.rs                 -- MemoryStore impl
│   │       ├── message.rs                -- MessageStore impl
│   │       └── context.rs                -- ContextStore impl
│   ├── orchy-store-sqlite/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                    -- SqliteBackend struct + init/migrations
│   │       ├── agent.rs                  -- AgentStore impl
│   │       ├── task.rs                   -- TaskStore impl
│   │       ├── memory.rs                 -- MemoryStore impl
│   │       ├── message.rs                -- MessageStore impl
│   │       └── context.rs                -- ContextStore impl
│   ├── orchy-store-pg/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                    -- PgBackend struct + init/migrations
│   │       ├── agent.rs                  -- AgentStore impl
│   │       ├── task.rs                   -- TaskStore impl
│   │       ├── memory.rs                 -- MemoryStore impl
│   │       ├── message.rs                -- MessageStore impl
│   │       └── context.rs                -- ContextStore impl
│   ├── orchy-server/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs                   -- entry point
│   │       ├── config.rs                 -- Config struct, TOML deser
│   │       ├── container.rs              -- Container assembly
│   │       ├── heartbeat.rs              -- background heartbeat monitor
│   │       └── mcp/
│   │           ├── mod.rs
│   │           ├── handler.rs            -- rmcp ServerHandler impl
│   │           └── tools.rs              -- 20 tool definitions
│   └── orchy-cli/
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs                    -- placeholder
```

---

### Task 1: Workspace scaffold and dependencies

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/orchy-core/Cargo.toml`
- Create: `crates/orchy-core/src/lib.rs`
- Create: `crates/orchy-store-memory/Cargo.toml`
- Create: `crates/orchy-store-memory/src/lib.rs`
- Create: `crates/orchy-store-sqlite/Cargo.toml`
- Create: `crates/orchy-store-sqlite/src/lib.rs`
- Create: `crates/orchy-store-pg/Cargo.toml`
- Create: `crates/orchy-store-pg/src/lib.rs`
- Create: `crates/orchy-server/Cargo.toml`
- Create: `crates/orchy-server/src/main.rs`
- Create: `crates/orchy-cli/Cargo.toml`
- Create: `crates/orchy-cli/src/lib.rs`
- Modify: `.gitignore`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/orchy-core",
    "crates/orchy-store-memory",
    "crates/orchy-store-sqlite",
    "crates/orchy-store-pg",
    "crates/orchy-server",
    "crates/orchy-cli",
]

[workspace.package]
edition = "2024"
version = "0.1.0"
license = "MIT"

[workspace.dependencies]
# Domain
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"

# Async
tokio = { version = "1", features = ["full"] }

# HTTP
reqwest = { version = "0.12", features = ["json"] }
axum = "0.8"

# MCP
rmcp = { version = "1.4", features = ["server", "macros", "transport-streamable-http-server"] }

# Storage
rusqlite = { version = "0.32", features = ["bundled", "fts5"] }
sqlite-vec = "0.1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
pgvector = { version = "0.4", features = ["sqlx"] }

# Config
toml = "0.8"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Internal
orchy-core = { path = "crates/orchy-core" }
orchy-store-memory = { path = "crates/orchy-store-memory" }
orchy-store-sqlite = { path = "crates/orchy-store-sqlite" }
orchy-store-pg = { path = "crates/orchy-store-pg" }
```

- [ ] **Step 2: Create orchy-core crate**

`crates/orchy-core/Cargo.toml`:
```toml
[package]
name = "orchy-core"
edition.workspace = true
version.workspace = true

[dependencies]
uuid.workspace = true
chrono.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
reqwest.workspace = true
```

`crates/orchy-core/src/lib.rs`:
```rust
pub mod embeddings;
pub mod entities;
pub mod error;
pub mod search;
pub mod services;
pub mod store;
pub mod value_objects;
```

- [ ] **Step 3: Create orchy-store-memory crate**

`crates/orchy-store-memory/Cargo.toml`:
```toml
[package]
name = "orchy-store-memory"
edition.workspace = true
version.workspace = true

[dependencies]
orchy-core.workspace = true
tokio.workspace = true
uuid.workspace = true
chrono.workspace = true
serde_json.workspace = true
tracing.workspace = true
```

`crates/orchy-store-memory/src/lib.rs`:
```rust
pub mod agent;
pub mod context;
pub mod memory;
pub mod message;
pub mod task;

use orchy_core::store::{AgentStore, ContextStore, MemoryStore, MessageStore, TaskStore};

pub struct MemoryBackend {
    // initialized in Task 8
}
```

- [ ] **Step 4: Create orchy-store-sqlite crate**

`crates/orchy-store-sqlite/Cargo.toml`:
```toml
[package]
name = "orchy-store-sqlite"
edition.workspace = true
version.workspace = true

[dependencies]
orchy-core.workspace = true
rusqlite.workspace = true
sqlite-vec.workspace = true
tokio.workspace = true
uuid.workspace = true
chrono.workspace = true
serde_json.workspace = true
tracing.workspace = true
```

`crates/orchy-store-sqlite/src/lib.rs`:
```rust
pub mod agent;
pub mod context;
pub mod memory;
pub mod message;
pub mod task;

pub struct SqliteBackend {
    // initialized in Task 9
}
```

- [ ] **Step 5: Create orchy-store-pg crate**

`crates/orchy-store-pg/Cargo.toml`:
```toml
[package]
name = "orchy-store-pg"
edition.workspace = true
version.workspace = true

[dependencies]
orchy-core.workspace = true
sqlx.workspace = true
pgvector.workspace = true
tokio.workspace = true
uuid.workspace = true
chrono.workspace = true
serde_json.workspace = true
tracing.workspace = true
```

`crates/orchy-store-pg/src/lib.rs`:
```rust
pub mod agent;
pub mod context;
pub mod memory;
pub mod message;
pub mod task;

pub struct PgBackend {
    // initialized in Task 10
}
```

- [ ] **Step 6: Create orchy-server crate**

`crates/orchy-server/Cargo.toml`:
```toml
[package]
name = "orchy-server"
edition.workspace = true
version.workspace = true

[dependencies]
orchy-core.workspace = true
orchy-store-memory.workspace = true
orchy-store-sqlite.workspace = true
orchy-store-pg.workspace = true
tokio.workspace = true
axum.workspace = true
rmcp.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

`crates/orchy-server/src/main.rs`:
```rust
fn main() {
    println!("orchy server");
}
```

- [ ] **Step 7: Create orchy-cli placeholder**

`crates/orchy-cli/Cargo.toml`:
```toml
[package]
name = "orchy-cli"
edition.workspace = true
version.workspace = true

[dependencies]
orchy-core.workspace = true
```

`crates/orchy-cli/src/lib.rs`:
```rust
// Future CLI client for orchy daemon
```

- [ ] **Step 8: Update .gitignore**

Append to existing `.gitignore`:
```
target/
*.db
*.db-shm
*.db-wal
.env
```

- [ ] **Step 9: Verify workspace compiles**

Run: `cargo build --workspace`
Expected: Compiles with warnings about unused modules/imports (expected at this stage).

- [ ] **Step 10: Commit**

```
git add -A
git commit -m "chore: scaffold workspace with 6 crates"
```

---

### Task 2: Value objects

**Files:**
- Create: `crates/orchy-core/src/error.rs`
- Create: `crates/orchy-core/src/value_objects/mod.rs`
- Create: `crates/orchy-core/src/value_objects/namespace.rs`
- Create: `crates/orchy-core/src/value_objects/ids.rs`
- Create: `crates/orchy-core/src/value_objects/priority.rs`
- Create: `crates/orchy-core/src/value_objects/task_status.rs`
- Create: `crates/orchy-core/src/value_objects/agent_status.rs`
- Create: `crates/orchy-core/src/value_objects/message_target.rs`
- Create: `crates/orchy-core/src/value_objects/version.rs`

- [ ] **Step 1: Write error module**

`crates/orchy-core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u64, actual: u64 },

    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("dependency not met: task {0} is not completed")]
    DependencyNotMet(String),

    #[error("store error: {0}")]
    Store(String),

    #[error("embeddings error: {0}")]
    Embeddings(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 2: Write Namespace value object**

`crates/orchy-core/src/value_objects/namespace.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);

impl Namespace {
    pub fn new<S: AsRef<str>>(value: S) -> Result<Self, Error> {
        let value = value.as_ref();

        if value.is_empty() {
            return Err(Error::InvalidInput("namespace cannot be empty".into()));
        }

        let parts: Vec<&str> = value.split('/').collect();

        for part in parts {
            if part.is_empty() {
                return Err(Error::InvalidInput(
                    "namespace cannot contain empty parts".into(),
                ));
            }

            if !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(Error::InvalidInput(
                    "namespace contains invalid characters".into(),
                ));
            }
        }

        Ok(Namespace(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this namespace starts with the given prefix.
    /// `project` matches `project/architecture`, `project/decisions`, etc.
    pub fn starts_with(&self, prefix: &Namespace) -> bool {
        self.0 == prefix.0 || self.0.starts_with(&format!("{}/", prefix.0))
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Namespace {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Namespace::new(value)
    }
}

impl From<Namespace> for String {
    fn from(ns: Namespace) -> Self {
        ns.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_simple() {
        let ns = Namespace::new("users").unwrap();
        assert_eq!(ns.as_str(), "users");
    }

    #[test]
    fn valid_hierarchical() {
        let ns = Namespace::new("project/architecture").unwrap();
        assert_eq!(ns.as_str(), "project/architecture");
    }

    #[test]
    fn valid_with_hyphens_and_underscores() {
        let ns = Namespace::new("user-accounts/premium_tier").unwrap();
        assert_eq!(ns.as_str(), "user-accounts/premium_tier");
    }

    #[test]
    fn empty_fails() {
        assert!(Namespace::new("").is_err());
    }

    #[test]
    fn empty_part_fails() {
        assert!(Namespace::new("users//accounts").is_err());
        assert!(Namespace::new("/users").is_err());
        assert!(Namespace::new("users/").is_err());
    }

    #[test]
    fn invalid_chars_fail() {
        assert!(Namespace::new("user@domain").is_err());
        assert!(Namespace::new("user.accounts").is_err());
        assert!(Namespace::new("user accounts").is_err());
    }

    #[test]
    fn starts_with_works() {
        let parent = Namespace::new("project").unwrap();
        let child = Namespace::new("project/architecture").unwrap();
        let other = Namespace::new("backend").unwrap();

        assert!(child.starts_with(&parent));
        assert!(parent.starts_with(&parent)); // exact match
        assert!(!other.starts_with(&parent));
    }
}
```

- [ ] **Step 3: Write ID value objects**

`crates/orchy-core/src/value_objects/ids.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

define_id!(AgentId);
define_id!(TaskId);
define_id!(MessageId);
define_id!(SnapshotId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let a = AgentId::new();
        let b = AgentId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn id_roundtrip_string() {
        let id = TaskId::new();
        let s = id.to_string();
        let parsed: TaskId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn id_roundtrip_json() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: MessageId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }
}
```

- [ ] **Step 4: Write Priority enum**

`crates/orchy-core/src/value_objects/priority.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Low => write!(f, "low"),
            Priority::Normal => write!(f, "normal"),
            Priority::High => write!(f, "high"),
            Priority::Critical => write!(f, "critical"),
        }
    }
}

impl std::str::FromStr for Priority {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Priority::Low),
            "normal" => Ok(Priority::Normal),
            "high" => Ok(Priority::High),
            "critical" => Ok(Priority::Critical),
            _ => Err(crate::error::Error::InvalidInput(format!(
                "invalid priority: {s}"
            ))),
        }
    }
}
```

- [ ] **Step 5: Write TaskStatus enum with transition enforcement**

`crates/orchy-core/src/value_objects/task_status.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Blocked,
    Claimed,
    InProgress,
    Completed,
    Failed,
}

impl TaskStatus {
    pub fn can_transition_to(&self, target: TaskStatus) -> bool {
        matches!(
            (self, target),
            (TaskStatus::Pending, TaskStatus::Claimed)
                | (TaskStatus::Pending, TaskStatus::Blocked)
                | (TaskStatus::Blocked, TaskStatus::Pending)
                | (TaskStatus::Claimed, TaskStatus::InProgress)
                | (TaskStatus::Claimed, TaskStatus::Pending) // release
                | (TaskStatus::InProgress, TaskStatus::Completed)
                | (TaskStatus::InProgress, TaskStatus::Failed)
                | (TaskStatus::InProgress, TaskStatus::Pending) // release
        )
    }

    pub fn transition_to(&self, target: TaskStatus) -> Result<TaskStatus, Error> {
        if self.can_transition_to(target) {
            Ok(target)
        } else {
            Err(Error::InvalidTransition {
                from: self.to_string(),
                to: target.to_string(),
            })
        }
    }
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Pending
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Blocked => write!(f, "blocked"),
            TaskStatus::Claimed => write!(f, "claimed"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(TaskStatus::Pending),
            "blocked" => Ok(TaskStatus::Blocked),
            "claimed" => Ok(TaskStatus::Claimed),
            "in_progress" => Ok(TaskStatus::InProgress),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            _ => Err(Error::InvalidInput(format!("invalid task status: {s}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transitions() {
        assert!(TaskStatus::Pending.can_transition_to(TaskStatus::Claimed));
        assert!(TaskStatus::Claimed.can_transition_to(TaskStatus::InProgress));
        assert!(TaskStatus::InProgress.can_transition_to(TaskStatus::Completed));
        assert!(TaskStatus::InProgress.can_transition_to(TaskStatus::Failed));
    }

    #[test]
    fn invalid_transitions() {
        assert!(!TaskStatus::Pending.can_transition_to(TaskStatus::Completed));
        assert!(!TaskStatus::Completed.can_transition_to(TaskStatus::Pending));
        assert!(!TaskStatus::Failed.can_transition_to(TaskStatus::InProgress));
    }

    #[test]
    fn release_transitions() {
        assert!(TaskStatus::Claimed.can_transition_to(TaskStatus::Pending));
        assert!(TaskStatus::InProgress.can_transition_to(TaskStatus::Pending));
    }
}
```

- [ ] **Step 6: Write AgentStatus enum**

`crates/orchy-core/src/value_objects/agent_status.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Online,
    Busy,
    Idle,
    Disconnected,
}

impl Default for AgentStatus {
    fn default() -> Self {
        AgentStatus::Online
    }
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentStatus::Online => write!(f, "online"),
            AgentStatus::Busy => write!(f, "busy"),
            AgentStatus::Idle => write!(f, "idle"),
            AgentStatus::Disconnected => write!(f, "disconnected"),
        }
    }
}
```

- [ ] **Step 7: Write MessageTarget enum**

`crates/orchy-core/src/value_objects/message_target.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::Error;
use crate::value_objects::ids::AgentId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum MessageTarget {
    Agent(AgentId),
    Role(String),
    Broadcast,
}

impl MessageTarget {
    pub fn parse(s: &str) -> Result<Self, Error> {
        if s == "broadcast" {
            Ok(MessageTarget::Broadcast)
        } else if let Some(role) = s.strip_prefix("role:") {
            if role.is_empty() {
                return Err(Error::InvalidInput("role name cannot be empty".into()));
            }
            Ok(MessageTarget::Role(role.to_string()))
        } else {
            let id: AgentId = s
                .parse()
                .map_err(|_| Error::InvalidInput(format!("invalid agent id: {s}")))?;
            Ok(MessageTarget::Agent(id))
        }
    }
}

impl fmt::Display for MessageTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageTarget::Agent(id) => write!(f, "{id}"),
            MessageTarget::Role(role) => write!(f, "role:{role}"),
            MessageTarget::Broadcast => write!(f, "broadcast"),
        }
    }
}

impl TryFrom<String> for MessageTarget {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        MessageTarget::parse(&value)
    }
}

impl From<MessageTarget> for String {
    fn from(target: MessageTarget) -> Self {
        target.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_broadcast() {
        let t = MessageTarget::parse("broadcast").unwrap();
        assert_eq!(t, MessageTarget::Broadcast);
    }

    #[test]
    fn parse_role() {
        let t = MessageTarget::parse("role:reviewer").unwrap();
        assert_eq!(t, MessageTarget::Role("reviewer".into()));
    }

    #[test]
    fn parse_agent_id() {
        let id = AgentId::new();
        let t = MessageTarget::parse(&id.to_string()).unwrap();
        assert_eq!(t, MessageTarget::Agent(id));
    }

    #[test]
    fn empty_role_fails() {
        assert!(MessageTarget::parse("role:").is_err());
    }
}
```

- [ ] **Step 8: Write Version value object**

`crates/orchy-core/src/value_objects/version.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(u64);

impl Version {
    pub fn initial() -> Self {
        Self(1)
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Version {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

- [ ] **Step 9: Write value_objects mod.rs**

`crates/orchy-core/src/value_objects/mod.rs`:
```rust
pub mod agent_status;
pub mod ids;
pub mod message_target;
pub mod namespace;
pub mod priority;
pub mod task_status;
pub mod version;

pub use agent_status::AgentStatus;
pub use ids::{AgentId, MessageId, SnapshotId, TaskId};
pub use message_target::MessageTarget;
pub use namespace::Namespace;
pub use priority::Priority;
pub use task_status::TaskStatus;
pub use version::Version;
```

- [ ] **Step 10: Run tests**

Run: `cargo test -p orchy-core`
Expected: All tests pass (namespace, ids, task_status, message_target).

- [ ] **Step 11: Commit**

```
git add -A
git commit -m "feat(core): add value objects — Namespace, IDs, Priority, TaskStatus, AgentStatus, MessageTarget, Version"
```

---

### Task 3: Entities and command objects

**Files:**
- Create: `crates/orchy-core/src/entities/mod.rs`
- Create: `crates/orchy-core/src/entities/agent.rs`
- Create: `crates/orchy-core/src/entities/task.rs`
- Create: `crates/orchy-core/src/entities/memory_entry.rs`
- Create: `crates/orchy-core/src/entities/message.rs`
- Create: `crates/orchy-core/src/entities/context.rs`

- [ ] **Step 1: Write Agent entity**

`crates/orchy-core/src/entities/agent.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::value_objects::{AgentId, AgentStatus, Namespace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub namespace: Option<Namespace>,
    pub roles: Vec<String>,
    pub description: String,
    pub status: AgentStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub connected_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RegisterAgent {
    pub namespace: Option<Namespace>,
    pub roles: Vec<String>,
    pub description: String,
    pub metadata: HashMap<String, String>,
}
```

- [ ] **Step 2: Write Task entity with filter and command**

`crates/orchy-core/src/entities/task.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, Namespace, Priority, TaskId, TaskStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub namespace: Namespace,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub claimed_by: Option<AgentId>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub depends_on: Vec<TaskId>,
    pub result_summary: Option<String>,
    pub created_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateTask {
    pub namespace: Namespace,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub depends_on: Vec<TaskId>,
    pub created_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub namespace: Option<Namespace>,
    pub status: Option<TaskStatus>,
    pub assigned_role: Option<String>,
    pub claimed_by: Option<AgentId>,
}
```

- [ ] **Step 3: Write MemoryEntry entity**

`crates/orchy-core/src/entities/memory_entry.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, Namespace, Version};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub namespace: Namespace,
    pub key: String,
    pub value: String,
    pub version: Version,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub written_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WriteMemory {
    pub namespace: Namespace,
    pub key: String,
    pub value: String,
    pub expected_version: Option<Version>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub written_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub namespace: Option<Namespace>,
}
```

- [ ] **Step 4: Write Message entity**

`crates/orchy-core/src/entities/message.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, MessageId, MessageTarget, Namespace};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    Pending,
    Delivered,
    Read,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub namespace: Option<Namespace>,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
    pub status: MessageStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateMessage {
    pub namespace: Option<Namespace>,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
}
```

- [ ] **Step 5: Write ContextSnapshot entity**

`crates/orchy-core/src/entities/context.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::value_objects::{AgentId, Namespace, SnapshotId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub id: SnapshotId,
    pub agent_id: AgentId,
    pub namespace: Option<Namespace>,
    pub summary: String,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateSnapshot {
    pub agent_id: AgentId,
    pub namespace: Option<Namespace>,
    pub summary: String,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub metadata: HashMap<String, String>,
}
```

- [ ] **Step 6: Write entities mod.rs**

`crates/orchy-core/src/entities/mod.rs`:
```rust
pub mod agent;
pub mod context;
pub mod memory_entry;
pub mod message;
pub mod task;

pub use agent::{Agent, RegisterAgent};
pub use context::{ContextSnapshot, CreateSnapshot};
pub use memory_entry::{MemoryEntry, MemoryFilter, WriteMemory};
pub use message::{CreateMessage, Message, MessageStatus};
pub use task::{CreateTask, Task, TaskFilter};
```

- [ ] **Step 7: Verify compilation**

Run: `cargo build -p orchy-core`
Expected: Compiles successfully.

- [ ] **Step 8: Commit**

```
git add -A
git commit -m "feat(core): add entities — Agent, Task, MemoryEntry, Message, ContextSnapshot"
```

---

### Task 4: Store traits and enum dispatch

**Files:**
- Create: `crates/orchy-core/src/store/task_store.rs`
- Create: `crates/orchy-core/src/store/memory_store.rs`
- Create: `crates/orchy-core/src/store/agent_store.rs`
- Create: `crates/orchy-core/src/store/message_store.rs`
- Create: `crates/orchy-core/src/store/context_store.rs`
- Create: `crates/orchy-core/src/store/mod.rs`

- [ ] **Step 1: Write TaskStore trait**

`crates/orchy-core/src/store/task_store.rs`:
```rust
use crate::entities::{CreateTask, Task, TaskFilter};
use crate::error::Result;
use crate::value_objects::{AgentId, TaskId};

pub trait TaskStore: Send + Sync {
    async fn create(&self, task: CreateTask) -> Result<Task>;
    async fn get(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>>;
    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task>;
    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task>;
    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task>;
    async fn release(&self, id: &TaskId) -> Result<Task>;
}
```

- [ ] **Step 2: Write MemoryStore trait**

`crates/orchy-core/src/store/memory_store.rs`:
```rust
use crate::entities::{MemoryEntry, MemoryFilter, WriteMemory};
use crate::error::Result;
use crate::value_objects::Namespace;

pub trait MemoryStore: Send + Sync {
    async fn write(&self, entry: WriteMemory) -> Result<MemoryEntry>;
    async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>>;
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>>;
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>>;
    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()>;
}
```

- [ ] **Step 3: Write AgentStore trait**

`crates/orchy-core/src/store/agent_store.rs`:
```rust
use crate::entities::{Agent, RegisterAgent};
use crate::error::Result;
use crate::value_objects::{AgentId, AgentStatus};

pub trait AgentStore: Send + Sync {
    async fn register(&self, registration: RegisterAgent) -> Result<Agent>;
    async fn get(&self, id: &AgentId) -> Result<Option<Agent>>;
    async fn list(&self) -> Result<Vec<Agent>>;
    async fn heartbeat(&self, id: &AgentId) -> Result<()>;
    async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()>;
    async fn disconnect(&self, id: &AgentId) -> Result<()>;
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>>;
}
```

- [ ] **Step 4: Write MessageStore trait**

`crates/orchy-core/src/store/message_store.rs`:
```rust
use crate::entities::{CreateMessage, Message};
use crate::error::Result;
use crate::value_objects::{AgentId, MessageId, Namespace};

pub trait MessageStore: Send + Sync {
    async fn send(&self, message: CreateMessage) -> Result<Message>;
    async fn check(&self, agent: &AgentId, namespace: Option<&Namespace>) -> Result<Vec<Message>>;
    async fn mark_read(&self, ids: &[MessageId]) -> Result<()>;
}
```

- [ ] **Step 5: Write ContextStore trait**

`crates/orchy-core/src/store/context_store.rs`:
```rust
use crate::entities::{ContextSnapshot, CreateSnapshot};
use crate::error::Result;
use crate::value_objects::{AgentId, Namespace};

pub trait ContextStore: Send + Sync {
    async fn save(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot>;
    async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>>;
    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>>;
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>>;
}
```

- [ ] **Step 6: Write store mod.rs with enum dispatch**

`crates/orchy-core/src/store/mod.rs`:
```rust
pub mod agent_store;
pub mod context_store;
pub mod memory_store;
pub mod message_store;
pub mod task_store;

pub use agent_store::AgentStore;
pub use context_store::ContextStore;
pub use memory_store::MemoryStore;
pub use message_store::MessageStore;
pub use task_store::TaskStore;

use crate::entities::*;
use crate::error::Result;
use crate::value_objects::*;

/// Enum dispatch for storage backends. Resolved once at startup.
/// Each variant wraps a backend that implements all store traits.
pub enum Store {
    // Variants added as backends are implemented:
    // Memory(orchy_store_memory::MemoryBackend),
    // Sqlite(orchy_store_sqlite::SqliteBackend),
    // Postgres(orchy_store_pg::PgBackend),
}

// Enum dispatch delegation will be implemented per-backend in Tasks 8-10.
// A macro will generate the match arms to forward each trait method.
```

- [ ] **Step 7: Verify compilation**

Run: `cargo build -p orchy-core`
Expected: Compiles successfully.

- [ ] **Step 8: Commit**

```
git add -A
git commit -m "feat(core): add store traits — TaskStore, MemoryStore, AgentStore, MessageStore, ContextStore"
```

---

### Task 5: Embeddings trait and OpenAI provider

**Files:**
- Create: `crates/orchy-core/src/embeddings/mod.rs`
- Create: `crates/orchy-core/src/embeddings/openai.rs`

- [ ] **Step 1: Write EmbeddingsProvider trait and enum dispatch**

`crates/orchy-core/src/embeddings/mod.rs`:
```rust
pub mod openai;

use crate::error::Result;
pub use openai::OpenAiEmbeddingsProvider;

pub trait EmbeddingsProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn model(&self) -> &str;
    fn dimensions(&self) -> u32;
}

pub enum EmbeddingsBackend {
    OpenAi(OpenAiEmbeddingsProvider),
}

impl EmbeddingsBackend {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.embed(text).await,
        }
    }

    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.embed_batch(texts).await,
        }
    }

    pub fn model(&self) -> &str {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.model(),
        }
    }

    pub fn dimensions(&self) -> u32 {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.dimensions(),
        }
    }
}
```

- [ ] **Step 2: Write OpenAI-compatible embeddings provider**

`crates/orchy-core/src/embeddings/openai.rs`:
```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub struct OpenAiEmbeddingsProvider {
    client: Client,
    url: String,
    model: String,
    dimensions: u32,
}

#[derive(Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: serde_json::Value,
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAiEmbeddingsProvider {
    pub fn new(url: String, model: String, dimensions: u32) -> Self {
        Self {
            client: Client::new(),
            url,
            model,
            dimensions,
        }
    }
}

impl super::EmbeddingsProvider for OpenAiEmbeddingsProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingsRequest {
            model: &self.model,
            input: serde_json::Value::String(text.to_string()),
        };

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        let body: EmbeddingsResponse = response
            .json()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        body.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| Error::Embeddings("empty response".into()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let input: Vec<serde_json::Value> = texts
            .iter()
            .map(|t| serde_json::Value::String(t.to_string()))
            .collect();

        let request = EmbeddingsRequest {
            model: &self.model,
            input: serde_json::Value::Array(input),
        };

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        let body: EmbeddingsResponse = response
            .json()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> u32 {
        self.dimensions
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build -p orchy-core`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```
git add -A
git commit -m "feat(core): add EmbeddingsProvider trait and OpenAI-compatible implementation"
```

---

### Task 6: Search utilities (RRF)

**Files:**
- Create: `crates/orchy-core/src/search.rs`

- [ ] **Step 1: Write RRF merge with tests**

`crates/orchy-core/src/search.rs`:
```rust
use std::collections::HashMap;
use std::hash::Hash;

const RRF_K: f64 = 60.0;

/// Reciprocal Rank Fusion — merges two ranked result lists.
/// Each result is scored as `1 / (k + rank)` per list, then summed.
/// Returns top `limit` results sorted by combined score descending.
pub fn reciprocal_rank_fusion<K: Eq + Hash + Clone, T: Clone>(
    keyword_results: &[(K, T)],
    vector_results: &[(K, T)],
    limit: usize,
) -> Vec<T> {
    let mut scores: HashMap<K, (f64, Option<T>)> = HashMap::new();

    for (rank, (key, item)) in keyword_results.iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores.entry(key.clone()).or_insert((0.0, None));
        entry.0 += score;
        if entry.1.is_none() {
            entry.1 = Some(item.clone());
        }
    }

    for (rank, (key, item)) in vector_results.iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = scores.entry(key.clone()).or_insert((0.0, None));
        entry.0 += score;
        if entry.1.is_none() {
            entry.1 = Some(item.clone());
        }
    }

    let mut combined: Vec<(f64, T)> = scores
        .into_values()
        .filter_map(|(score, item)| item.map(|i| (score, i)))
        .collect();

    combined.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    combined.truncate(limit);
    combined.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_merges_two_lists() {
        let keyword: Vec<(u32, &str)> = vec![(1, "a"), (2, "b"), (3, "c")];
        let vector: Vec<(u32, &str)> = vec![(2, "b"), (4, "d"), (1, "a")];

        let results = reciprocal_rank_fusion(&keyword, &vector, 3);

        // "b" appears at rank 1 in keyword (score 1/62) and rank 0 in vector (score 1/61)
        // "a" appears at rank 0 in keyword (score 1/61) and rank 2 in vector (score 1/63)
        // "b" should have highest combined score
        assert_eq!(results[0], "b");
        assert_eq!(results[1], "a");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn rrf_respects_limit() {
        let keyword: Vec<(u32, &str)> = vec![(1, "a"), (2, "b"), (3, "c")];
        let vector: Vec<(u32, &str)> = vec![];

        let results = reciprocal_rank_fusion(&keyword, &vector, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn rrf_empty_inputs() {
        let keyword: Vec<(u32, &str)> = vec![];
        let vector: Vec<(u32, &str)> = vec![];

        let results = reciprocal_rank_fusion(&keyword, &vector, 10);
        assert!(results.is_empty());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p orchy-core`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```
git add -A
git commit -m "feat(core): add Reciprocal Rank Fusion search utility"
```

---

### Task 7: Services

**Files:**
- Create: `crates/orchy-core/src/services/mod.rs`
- Create: `crates/orchy-core/src/services/agent_service.rs`
- Create: `crates/orchy-core/src/services/task_service.rs`
- Create: `crates/orchy-core/src/services/memory_service.rs`
- Create: `crates/orchy-core/src/services/message_service.rs`
- Create: `crates/orchy-core/src/services/context_service.rs`

- [ ] **Step 1: Write AgentService**

`crates/orchy-core/src/services/agent_service.rs`:
```rust
use crate::entities::{Agent, RegisterAgent};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::{AgentId, AgentStatus};

pub struct AgentService {
    store: Store,
}

impl AgentService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        self.store.register(registration).await
    }

    pub async fn get(&self, id: &AgentId) -> Result<Agent> {
        self.store
            .get_agent(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))
    }

    pub async fn list(&self) -> Result<Vec<Agent>> {
        self.store.list_agents().await
    }

    pub async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        self.store.heartbeat(id).await
    }

    pub async fn disconnect(&self, id: &AgentId) -> Result<()> {
        self.store.disconnect(id).await
    }

    pub async fn disconnect_timed_out(&self, timeout_secs: u64) -> Result<Vec<AgentId>> {
        let agents = self.store.find_timed_out(timeout_secs).await?;
        let ids: Vec<AgentId> = agents.iter().map(|a| a.id.clone()).collect();
        for id in &ids {
            self.store.disconnect(id).await?;
        }
        Ok(ids)
    }
}
```

- [ ] **Step 2: Write TaskService with invariant enforcement**

`crates/orchy-core/src/services/task_service.rs`:
```rust
use crate::entities::{CreateTask, Task, TaskFilter};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::{AgentId, Namespace, TaskId, TaskStatus};

pub struct TaskService {
    store: Store,
}

impl TaskService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub async fn create(&self, cmd: CreateTask) -> Result<Task> {
        // If task has dependencies, check they exist
        for dep_id in &cmd.depends_on {
            let dep = self
                .store
                .get_task(dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            // If dependency is not completed, new task starts as Blocked
            if dep.status != TaskStatus::Completed {
                // Task will be created as Blocked — store handles initial status
            }
        }
        self.store.create_task(cmd).await
    }

    pub async fn get(&self, id: &TaskId) -> Result<Task> {
        self.store
            .get_task(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    pub async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        self.store.list_tasks(filter).await
    }

    pub async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let task = self.get(id).await?;

        if task.status != TaskStatus::Pending {
            return Err(Error::Conflict(format!(
                "task {id} is {}, not pending",
                task.status
            )));
        }

        // Verify all dependencies are completed
        for dep_id in &task.depends_on {
            let dep = self.get(dep_id).await?;
            if dep.status != TaskStatus::Completed {
                return Err(Error::DependencyNotMet(dep_id.to_string()));
            }
        }

        self.store.claim_task(id, agent).await
    }

    pub async fn get_next(
        &self,
        agent: &AgentId,
        namespace: Option<&Namespace>,
        role: Option<&str>,
    ) -> Result<Option<Task>> {
        // List pending tasks, optionally filtered
        let filter = TaskFilter {
            namespace: namespace.cloned(),
            status: Some(TaskStatus::Pending),
            assigned_role: role.map(String::from),
            ..Default::default()
        };

        let tasks = self.store.list_tasks(filter).await?;

        // Find first task whose dependencies are all met
        for task in tasks {
            let deps_met = self.all_deps_completed(&task.depends_on).await?;
            if deps_met {
                return self.store.claim_task(&task.id, agent).await.map(Some);
            }
        }

        Ok(None)
    }

    pub async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let task = self.store.complete_task(id, summary).await?;

        // Unblock dependent tasks
        self.unblock_dependents(id).await?;

        Ok(task)
    }

    pub async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        self.store.fail_task(id, reason).await
    }

    pub async fn release(&self, id: &TaskId) -> Result<Task> {
        self.store.release_task(id).await
    }

    pub async fn release_agent_tasks(&self, agent: &AgentId) -> Result<Vec<TaskId>> {
        let filter = TaskFilter {
            claimed_by: Some(agent.clone()),
            ..Default::default()
        };

        let tasks = self.store.list_tasks(filter).await?;
        let mut released = Vec::new();

        for task in tasks {
            if task.status == TaskStatus::Claimed || task.status == TaskStatus::InProgress {
                self.store.release_task(&task.id).await?;
                released.push(task.id);
            }
        }

        Ok(released)
    }

    async fn all_deps_completed(&self, deps: &[TaskId]) -> Result<bool> {
        for dep_id in deps {
            if let Some(dep) = self.store.get_task(dep_id).await? {
                if dep.status != TaskStatus::Completed {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    async fn unblock_dependents(&self, completed_id: &TaskId) -> Result<()> {
        let all_tasks = self.store.list_tasks(TaskFilter::default()).await?;

        for task in all_tasks {
            if task.status == TaskStatus::Blocked && task.depends_on.contains(completed_id) {
                if self.all_deps_completed(&task.depends_on).await? {
                    self.store
                        .update_task_status(&task.id, TaskStatus::Pending)
                        .await?;
                }
            }
        }

        Ok(())
    }
}
```

Note: `Store` will need `update_task_status` — add to `TaskStore` trait:
```rust
async fn update_task_status(&self, id: &TaskId, status: TaskStatus) -> Result<()>;
```

- [ ] **Step 3: Write MemoryService with embeddings integration**

`crates/orchy-core/src/services/memory_service.rs`:
```rust
use crate::embeddings::EmbeddingsBackend;
use crate::entities::{MemoryEntry, MemoryFilter, WriteMemory};
use crate::error::Result;
use crate::search::reciprocal_rank_fusion;
use crate::store::Store;
use crate::value_objects::{AgentId, Namespace};

pub struct MemoryService {
    store: Store,
    embeddings: Option<EmbeddingsBackend>,
}

impl MemoryService {
    pub fn new(store: Store, embeddings: Option<EmbeddingsBackend>) -> Self {
        Self { store, embeddings }
    }

    pub async fn write(
        &self,
        mut cmd: WriteMemory,
        written_by: Option<AgentId>,
    ) -> Result<MemoryEntry> {
        cmd.written_by = written_by;

        // Generate embedding if provider is configured
        if let Some(ref embeddings) = self.embeddings {
            let vector = embeddings.embed(&cmd.value).await?;
            cmd.embedding = Some(vector);
            cmd.embedding_model = Some(embeddings.model().to_string());
            cmd.embedding_dimensions = Some(embeddings.dimensions());
        }

        self.store.write_memory(cmd).await
    }

    pub async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        self.store.read_memory(namespace, key).await
    }

    pub async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        self.store.list_memory(filter).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        // Generate query embedding if available
        let embedding = match &self.embeddings {
            Some(emb) => Some(emb.embed(query).await?),
            None => None,
        };

        self.store
            .search_memory(query, embedding.as_deref(), namespace, limit)
            .await
    }

    pub async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        self.store.delete_memory(namespace, key).await
    }
}
```

- [ ] **Step 4: Write MessageService with fan-out**

`crates/orchy-core/src/services/message_service.rs`:
```rust
use crate::entities::{CreateMessage, Message};
use crate::error::Result;
use crate::store::Store;
use crate::value_objects::{AgentId, MessageId, MessageTarget, Namespace};

pub struct MessageService {
    store: Store,
}

impl MessageService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub async fn send(&self, cmd: CreateMessage) -> Result<Vec<Message>> {
        match &cmd.to {
            MessageTarget::Agent(_) => {
                let msg = self.store.send_message(cmd).await?;
                Ok(vec![msg])
            }
            MessageTarget::Role(role) => {
                // Fan out to all agents with this role
                let agents = self.store.list_agents().await?;
                let mut messages = Vec::new();

                for agent in agents {
                    if agent.roles.contains(role) {
                        let msg_cmd = CreateMessage {
                            namespace: cmd.namespace.clone(),
                            from: cmd.from.clone(),
                            to: MessageTarget::Agent(agent.id),
                            body: cmd.body.clone(),
                        };
                        messages.push(self.store.send_message(msg_cmd).await?);
                    }
                }

                Ok(messages)
            }
            MessageTarget::Broadcast => {
                let agents = self.store.list_agents().await?;
                let mut messages = Vec::new();

                for agent in agents {
                    if agent.id != cmd.from {
                        let msg_cmd = CreateMessage {
                            namespace: cmd.namespace.clone(),
                            from: cmd.from.clone(),
                            to: MessageTarget::Agent(agent.id),
                            body: cmd.body.clone(),
                        };
                        messages.push(self.store.send_message(msg_cmd).await?);
                    }
                }

                Ok(messages)
            }
        }
    }

    pub async fn check(
        &self,
        agent: &AgentId,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<Message>> {
        self.store.check_messages(agent, namespace).await
    }

    pub async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        self.store.mark_messages_read(ids).await
    }
}
```

- [ ] **Step 5: Write ContextService with embeddings integration**

`crates/orchy-core/src/services/context_service.rs`:
```rust
use crate::embeddings::EmbeddingsBackend;
use crate::entities::{ContextSnapshot, CreateSnapshot};
use crate::error::Result;
use crate::store::Store;
use crate::value_objects::{AgentId, Namespace};

pub struct ContextService {
    store: Store,
    embeddings: Option<EmbeddingsBackend>,
}

impl ContextService {
    pub fn new(store: Store, embeddings: Option<EmbeddingsBackend>) -> Self {
        Self { store, embeddings }
    }

    pub async fn save(&self, mut cmd: CreateSnapshot) -> Result<ContextSnapshot> {
        if let Some(ref embeddings) = self.embeddings {
            let vector = embeddings.embed(&cmd.summary).await?;
            cmd.embedding = Some(vector);
            cmd.embedding_model = Some(embeddings.model().to_string());
            cmd.embedding_dimensions = Some(embeddings.dimensions());
        }

        self.store.save_context(cmd).await
    }

    pub async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        self.store.load_context(agent).await
    }

    pub async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>> {
        self.store.list_contexts(agent, namespace).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let embedding = match &self.embeddings {
            Some(emb) => Some(emb.embed(query).await?),
            None => None,
        };

        self.store
            .search_contexts(query, embedding.as_deref(), namespace, agent_id, limit)
            .await
    }
}
```

- [ ] **Step 6: Write services mod.rs**

`crates/orchy-core/src/services/mod.rs`:
```rust
pub mod agent_service;
pub mod context_service;
pub mod memory_service;
pub mod message_service;
pub mod task_service;

pub use agent_service::AgentService;
pub use context_service::ContextService;
pub use memory_service::MemoryService;
pub use message_service::MessageService;
pub use task_service::TaskService;
```

- [ ] **Step 7: Update Store enum with method aliases**

The services call methods like `self.store.create_task()` but `Store` enum needs to
delegate these. Update `crates/orchy-core/src/store/mod.rs` to add the delegation
methods. For now, add the method signatures with `todo!()` bodies — they'll be
implemented when backends are wired in Tasks 8-10.

Also update `TaskStore` trait to include `update_task_status`:
```rust
async fn update_task_status(&self, id: &TaskId, status: TaskStatus) -> Result<()>;
```

- [ ] **Step 8: Verify compilation**

Run: `cargo build -p orchy-core`
Expected: Compiles (with warnings about unused code — expected, backends not wired yet).

- [ ] **Step 9: Commit**

```
git add -A
git commit -m "feat(core): add services — TaskService, MemoryService, AgentService, MessageService, ContextService"
```

---

### Task 8: In-memory store backend

**Files:**
- Modify: `crates/orchy-store-memory/src/lib.rs`
- Create: `crates/orchy-store-memory/src/agent.rs`
- Create: `crates/orchy-store-memory/src/task.rs`
- Create: `crates/orchy-store-memory/src/memory.rs`
- Create: `crates/orchy-store-memory/src/message.rs`
- Create: `crates/orchy-store-memory/src/context.rs`

This task implements all 5 store traits using `HashMap + RwLock`. Search uses
substring matching, or cosine similarity when embeddings are present.

- [ ] **Step 1: Implement MemoryBackend struct**

`crates/orchy-store-memory/src/lib.rs`:
```rust
pub mod agent;
pub mod context;
pub mod memory;
pub mod message;
pub mod task;

use std::collections::HashMap;
use std::sync::RwLock;

use orchy_core::entities::*;
use orchy_core::value_objects::*;

pub struct MemoryBackend {
    pub(crate) agents: RwLock<HashMap<AgentId, Agent>>,
    pub(crate) tasks: RwLock<HashMap<TaskId, Task>>,
    pub(crate) memory: RwLock<HashMap<(String, String), MemoryEntry>>, // (namespace, key)
    pub(crate) messages: RwLock<HashMap<MessageId, Message>>,
    pub(crate) contexts: RwLock<HashMap<SnapshotId, ContextSnapshot>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            memory: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            contexts: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Implement AgentStore for MemoryBackend**

Implement in `crates/orchy-store-memory/src/agent.rs`. Full CRUD using
`self.agents` RwLock. `find_timed_out` checks `last_heartbeat` against
`chrono::Utc::now()`.

- [ ] **Step 3: Implement TaskStore for MemoryBackend**

Implement in `crates/orchy-store-memory/src/task.rs`. Full CRUD. `claim` sets
`claimed_by` and `claimed_at`, transitions `Pending → Claimed`. `create` sets
initial status to `Blocked` if any dependency is not `Completed`, else `Pending`.
Tasks sorted by priority (descending) in `list`.

- [ ] **Step 4: Implement MemoryStore for MemoryBackend**

Implement in `crates/orchy-store-memory/src/memory.rs`. `write` checks version
for optimistic concurrency. `search` does substring matching on `value` field;
if `embedding` is provided, computes cosine similarity and combines results.
`list` with namespace prefix filtering using `Namespace::starts_with`.

- [ ] **Step 5: Implement MessageStore for MemoryBackend**

Implement in `crates/orchy-store-memory/src/message.rs`. `check` finds messages
where `to` matches the agent (directly or via role/broadcast) and status is
`Pending`, transitions to `Delivered`.

- [ ] **Step 6: Implement ContextStore for MemoryBackend**

Implement in `crates/orchy-store-memory/src/context.rs`. `load` returns the most
recent snapshot for the agent. `search` does substring match on `summary`; with
embedding, does cosine similarity.

- [ ] **Step 7: Wire MemoryBackend into Store enum**

Update `crates/orchy-core/src/store/mod.rs`: add `Memory(MemoryBackend)` variant
and implement delegation for all trait methods.

- [ ] **Step 8: Write integration tests**

Create `crates/orchy-store-memory/tests/integration.rs` with tests covering:
- Agent register, heartbeat, disconnect
- Task create, claim, complete, dependency blocking/unblocking
- Memory write, read, optimistic concurrency rejection, search
- Message send, check, fan-out
- Context save, load, search

Run: `cargo test -p orchy-store-memory`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```
git add -A
git commit -m "feat(store-memory): implement in-memory storage backend with all store traits"
```

---

### Task 9: SQLite store backend

**Files:**
- Modify: `crates/orchy-store-sqlite/src/lib.rs`
- Create: `crates/orchy-store-sqlite/src/agent.rs`
- Create: `crates/orchy-store-sqlite/src/task.rs`
- Create: `crates/orchy-store-sqlite/src/memory.rs`
- Create: `crates/orchy-store-sqlite/src/message.rs`
- Create: `crates/orchy-store-sqlite/src/context.rs`

- [ ] **Step 1: Implement SqliteBackend with schema initialization**

`crates/orchy-store-sqlite/src/lib.rs`: Create `SqliteBackend` with a
`rusqlite::Connection` wrapped in `tokio::sync::Mutex` (rusqlite is not async).
On `new()`, run the full DDL from the spec: create all tables, FTS5 virtual
tables, sqlite-vec virtual tables (with dimension from config parameter).
Register sqlite-vec extension via `sqlite3_auto_extension`.

- [ ] **Step 2: Implement AgentStore for SqliteBackend**

Raw SQL INSERT/SELECT/UPDATE in `crates/orchy-store-sqlite/src/agent.rs`.

- [ ] **Step 3: Implement TaskStore for SqliteBackend**

Raw SQL in `crates/orchy-store-sqlite/src/task.rs`. `depends_on` and
`assigned_roles` stored as JSON text, parsed with `serde_json`.

- [ ] **Step 4: Implement MemoryStore for SqliteBackend**

Raw SQL in `crates/orchy-store-sqlite/src/memory.rs`. `write` uses
`INSERT OR REPLACE` with version check. `search` runs FTS5 query
(`SELECT ... FROM memory_fts WHERE memory_fts MATCH ?`) ranked by `bm25()`.
When embedding provided, also queries `memory_vec` with
`SELECT ... ORDER BY vec_distance_cosine(embedding, ?) LIMIT ?`.
Returns both result sets to the service layer for RRF merge.

- [ ] **Step 5: Implement MessageStore and ContextStore for SqliteBackend**

Raw SQL in `message.rs` and `context.rs`. Context search uses FTS5 + vec0
same pattern as memory.

- [ ] **Step 6: Wire SqliteBackend into Store enum**

Add `Sqlite(SqliteBackend)` variant to `Store` enum, implement delegation.

- [ ] **Step 7: Write integration tests**

Same test coverage as Task 8 but against SQLite. Use a temporary in-memory
SQLite database (`":memory:"`).

Run: `cargo test -p orchy-store-sqlite`
Expected: All tests pass.

- [ ] **Step 8: Commit**

```
git add -A
git commit -m "feat(store-sqlite): implement SQLite storage backend with FTS5 and sqlite-vec"
```

---

### Task 10: Postgres store backend

**Files:**
- Modify: `crates/orchy-store-pg/src/lib.rs`
- Create: `crates/orchy-store-pg/src/agent.rs`
- Create: `crates/orchy-store-pg/src/task.rs`
- Create: `crates/orchy-store-pg/src/memory.rs`
- Create: `crates/orchy-store-pg/src/message.rs`
- Create: `crates/orchy-store-pg/src/context.rs`

- [ ] **Step 1: Implement PgBackend with schema initialization**

`crates/orchy-store-pg/src/lib.rs`: Create `PgBackend` with a `sqlx::PgPool`.
On `new()`, run migrations (all DDL from the spec Postgres section).
Dynamically create HNSW indexes if embeddings dimension is provided.

- [ ] **Step 2: Implement all store traits using raw sqlx queries**

Each file (`agent.rs`, `task.rs`, `memory.rs`, `message.rs`, `context.rs`)
uses `sqlx::query!` or `sqlx::query_as!` with raw SQL. Memory search uses
`to_tsvector`/`ts_rank` for keyword and `<=>` operator for pgvector cosine
distance.

- [ ] **Step 3: Wire PgBackend into Store enum**

Add `Postgres(PgBackend)` variant to `Store` enum, implement delegation.

- [ ] **Step 4: Write integration tests**

Tests require a running Postgres instance. Use `compose.yaml` to start it.
Tests create a fresh schema per test run (random schema name for isolation).

Run: `just db-up && cargo test -p orchy-store-pg`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```
git add -A
git commit -m "feat(store-pg): implement Postgres storage backend with pgvector"
```

---

### Task 11: Server config and container

**Files:**
- Create: `crates/orchy-server/src/config.rs`
- Create: `crates/orchy-server/src/container.rs`
- Create: `config.toml`

- [ ] **Step 1: Write Config struct**

`crates/orchy-server/src/config.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub store: StoreConfig,
    pub embeddings: Option<EmbeddingsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u64,
}

fn default_heartbeat_timeout() -> u64 {
    60
}

#[derive(Debug, Deserialize)]
pub struct StoreConfig {
    pub backend: String,
    pub sqlite: Option<SqliteConfig>,
    pub postgres: Option<PostgresConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SqliteConfig {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingsConfig {
    pub provider: String,
    pub openai: Option<OpenAiEmbeddingsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiEmbeddingsConfig {
    pub url: String,
    pub model: String,
    pub dimensions: u32,
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
```

- [ ] **Step 2: Write Container**

`crates/orchy-server/src/container.rs`:
```rust
use std::sync::Arc;

use orchy_core::embeddings::{EmbeddingsBackend, OpenAiEmbeddingsProvider};
use orchy_core::services::*;
use orchy_core::store::Store;

use crate::config::Config;

pub struct Container {
    pub task_service: TaskService,
    pub memory_service: MemoryService,
    pub agent_service: AgentService,
    pub message_service: MessageService,
    pub context_service: ContextService,
    pub config: Config,
}

impl Container {
    pub async fn new(config: Config) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        // Build store
        let store = match config.store.backend.as_str() {
            "memory" => Store::Memory(orchy_store_memory::MemoryBackend::new()),
            "sqlite" => {
                let sqlite_config = config
                    .store
                    .sqlite
                    .as_ref()
                    .expect("sqlite config required when backend=sqlite");
                let dimensions = config
                    .embeddings
                    .as_ref()
                    .and_then(|e| e.openai.as_ref())
                    .map(|o| o.dimensions);
                Store::Sqlite(
                    orchy_store_sqlite::SqliteBackend::new(&sqlite_config.path, dimensions)
                        .await?,
                )
            }
            "postgres" => {
                let pg_config = config
                    .store
                    .postgres
                    .as_ref()
                    .expect("postgres config required when backend=postgres");
                let dimensions = config
                    .embeddings
                    .as_ref()
                    .and_then(|e| e.openai.as_ref())
                    .map(|o| o.dimensions);
                Store::Postgres(
                    orchy_store_pg::PgBackend::new(&pg_config.url, dimensions).await?,
                )
            }
            other => panic!("unknown store backend: {other}"),
        };

        // Build embeddings
        let embeddings = config.embeddings.as_ref().map(|emb_config| {
            match emb_config.provider.as_str() {
                "openai" => {
                    let openai = emb_config
                        .openai
                        .as_ref()
                        .expect("openai config required when provider=openai");
                    EmbeddingsBackend::OpenAi(OpenAiEmbeddingsProvider::new(
                        openai.url.clone(),
                        openai.model.clone(),
                        openai.dimensions,
                    ))
                }
                other => panic!("unknown embeddings provider: {other}"),
            }
        });

        // Assemble services
        // Note: Store does not implement Clone. Services need shared access.
        // Wrap store in Arc and share across services.
        // This requires refactoring services to use Arc<Store> instead of Store.
        // Alternatively, since Container owns everything and is passed as Arc<Container>,
        // services can hold references — but this gets complex with lifetimes.
        // Simplest: services hold Arc<Store>, embeddings hold Arc<EmbeddingsBackend>.

        let store = Arc::new(store);
        let embeddings = embeddings.map(Arc::new);

        // Services will need to accept Arc<Store> — adjust service constructors
        // in the implementation to use Arc<Store> instead of Store.

        Ok(Arc::new(Self {
            task_service: TaskService::new(store.clone()),
            memory_service: MemoryService::new(store.clone(), embeddings.clone()),
            agent_service: AgentService::new(store.clone()),
            message_service: MessageService::new(store.clone()),
            context_service: ContextService::new(store.clone(), embeddings),
            config,
        }))
    }
}
```

Note: The services in Task 7 use `Store` directly. During implementation, change
them to use `Arc<Store>` so the `Container` can share a single store instance
across all services.

- [ ] **Step 3: Write default config.toml**

`config.toml`:
```toml
[server]
host = "127.0.0.1"
port = 3100
heartbeat_timeout_secs = 60

[store]
backend = "memory"

# [store.sqlite]
# path = "orchy.db"

# [store.postgres]
# url = "postgres://orchy:orchy@localhost:5432/orchy"

# [embeddings]
# provider = "openai"
#
# [embeddings.openai]
# url = "http://localhost:11434/v1/embeddings"
# model = "nomic-embed-text"
# dimensions = 768
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p orchy-server`
Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```
git add -A
git commit -m "feat(server): add Config and Container for startup assembly"
```

---

### Task 12: MCP handler and tool definitions

**Files:**
- Create: `crates/orchy-server/src/mcp/mod.rs`
- Create: `crates/orchy-server/src/mcp/handler.rs`
- Create: `crates/orchy-server/src/mcp/tools.rs`

This task implements the rmcp `ServerHandler` with all 20 MCP tools.

- [ ] **Step 1: Define the MCP handler struct**

`crates/orchy-server/src/mcp/handler.rs`:
The handler struct holds `Arc<Container>` and a session-bound `Option<AgentId>`.
Implement `rmcp::ServerHandler` with `get_info()` returning server metadata.
Each `#[tool]` method delegates to the corresponding service on the container.

Tools to implement (20 total):
- `register_agent`, `list_agents`, `heartbeat`
- `post_task`, `get_next_task`, `list_tasks`, `claim_task`, `complete_task`, `fail_task`
- `write_memory`, `read_memory`, `list_memory`, `search_memory`, `delete_memory`
- `send_message`, `check_mailbox`, `mark_read`
- `save_context`, `load_context`, `list_contexts`, `search_contexts`

Each tool method:
1. Deserializes params from the `#[tool(aggr)]` argument struct
2. Validates and converts string params to value objects (Namespace, TaskId, etc.)
3. Calls the service method
4. Serializes the result to JSON string for the tool response

- [ ] **Step 2: Define tool argument structs**

`crates/orchy-server/src/mcp/tools.rs`:
One struct per tool with `serde::Deserialize`:
```rust
#[derive(Deserialize)]
pub struct RegisterAgentArgs {
    pub roles: Vec<String>,
    pub description: String,
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct PostTaskArgs {
    pub namespace: String,
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

// ... one struct per tool (20 total)
```

- [ ] **Step 3: Write mcp/mod.rs**

```rust
pub mod handler;
pub mod tools;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build -p orchy-server`
Expected: Compiles.

- [ ] **Step 5: Commit**

```
git add -A
git commit -m "feat(server): add MCP handler with 20 tool definitions"
```

---

### Task 13: Server main and heartbeat monitor

**Files:**
- Modify: `crates/orchy-server/src/main.rs`
- Create: `crates/orchy-server/src/heartbeat.rs`

- [ ] **Step 1: Write heartbeat monitor**

`crates/orchy-server/src/heartbeat.rs`:
```rust
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::info;

use crate::container::Container;

pub async fn run_heartbeat_monitor(container: Arc<Container>) {
    let timeout = container.config.server.heartbeat_timeout_secs;
    let check_interval = Duration::from_secs(timeout / 2);

    let mut ticker = interval(check_interval);

    loop {
        ticker.tick().await;

        match container.agent_service.disconnect_timed_out(timeout).await {
            Ok(disconnected) => {
                for agent_id in &disconnected {
                    info!(%agent_id, "agent timed out, disconnecting");
                    if let Err(e) = container.task_service.release_agent_tasks(agent_id).await {
                        tracing::error!(%agent_id, error = %e, "failed to release agent tasks");
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "heartbeat check failed");
            }
        }
    }
}
```

- [ ] **Step 2: Write main.rs**

`crates/orchy-server/src/main.rs`:
```rust
mod config;
mod container;
mod heartbeat;
mod mcp;

use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpService,
};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::container::Container;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config = Config::load(&config_path)?;
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);

    let container = Container::new(config).await?;

    // Start heartbeat monitor
    let hb_container = container.clone();
    tokio::spawn(async move {
        heartbeat::run_heartbeat_monitor(hb_container).await;
    });

    // Build MCP service
    let mcp_container = container.clone();
    let service = StreamableHttpService::new(
        move || Ok(mcp::handler::OrchyHandler::new(mcp_container.clone())),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    let router = axum::Router::new().nest_service("/mcp", service);

    tracing::info!("orchy server listening on {bind_addr}");

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
```

- [ ] **Step 3: Verify the server starts**

Run: `cargo run -p orchy-server`
Expected: Server prints "orchy server listening on 127.0.0.1:3100" and runs
with in-memory backend (default config).

- [ ] **Step 4: Commit**

```
git add -A
git commit -m "feat(server): add main entrypoint with heartbeat monitor and MCP transport"
```

---

### Task 14: End-to-end smoke test

**Files:**
- Create: `tests/smoke.rs` or test via curl commands

- [ ] **Step 1: Manual smoke test via curl**

Start the server:
```bash
cargo run -p orchy-server &
```

Test MCP initialize:
```bash
curl -X POST http://localhost:3100/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}'
```

Expected: JSON response with `serverInfo` and `Mcp-Session-Id` header.

Test register_agent (use session ID from above):
```bash
curl -X POST http://localhost:3100/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "Mcp-Session-Id: <session-id>" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"register_agent","arguments":{"roles":["rust","review"],"description":"test agent"}}}'
```

Expected: JSON response with agent ID, roles, status "online".

Test post_task and list_tasks similarly.

- [ ] **Step 2: Kill the server and commit**

```
kill %1
git add -A
git commit -m "test: verify end-to-end MCP transport with in-memory backend"
```

---

## Summary

| Task | Description | Dependencies |
|------|-------------|-------------|
| 1 | Workspace scaffold | — |
| 2 | Value objects | 1 |
| 3 | Entities | 2 |
| 4 | Store traits + enum dispatch | 3 |
| 5 | Embeddings trait + OpenAI provider | 2 |
| 6 | Search utilities (RRF) | 2 |
| 7 | Services | 4, 5, 6 |
| 8 | In-memory store backend | 4 |
| 9 | SQLite store backend | 4 |
| 10 | Postgres store backend | 4 |
| 11 | Server config + container | 7, 8, 9, 10 |
| 12 | MCP handler + tools | 7 |
| 13 | Server main + heartbeat | 11, 12 |
| 14 | End-to-end smoke test | 13 |
