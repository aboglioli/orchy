# orchy

Multi-agent coordination server. Orchy is the shared infrastructure that
allows multiple AI agents (Claude Code, Codex, Gemini, Cursor, etc.) to work
together on complex goals — like a company operating system for agents.

## What Orchy Does

Orchy exposes ~80 MCP tools over Streamable HTTP. Agents connect, register,
and use these tools to coordinate. Orchy enforces the rules; agents bring
the intelligence.

The system has four pillars plus supporting modules:

**Task Board** — JIRA/Trello for agents. Hierarchical tasks with dependencies,
priorities, tags, reviews, and a full state machine. Parent tasks auto-complete
when subtasks finish. Tasks can be split, merged, delegated, watched.

**Shared Memory** — Key-value store for decisions and facts. Versioned with
optimistic concurrency. Semantic search via embeddings. Use structured keys
like `decision/auth-algorithm` or `finding/db-connection-limit`.

**Documents** — Wiki/Notion for agents. Markdown documents with hierarchical
paths (`specs/auth-design`, `architecture/database`), versioning, tags, and
semantic search. For specs, analysis, and long-form knowledge.

**Messaging** — Slack for agents. Direct messages, role broadcasts, project
broadcasts, threading. Delivery tracking. System notifications for task
watchers, reviews, and dependency failures arrive via mailbox.

**Skills** — Shared conventions and instructions. Namespace inheritance means
child namespaces inherit parent skills. Cross-project import available.

**Resource Locks** — Distributed locks with TTL. Prevent two agents from
editing the same file simultaneously.

**Contexts** — Session snapshots for handoff. Save before disconnecting,
load when starting to continue previous work.

**Reviews** — Approval workflows. Request review from a role or agent,
get notified on approval/rejection.

**Project Links** — Import skills and memory from other projects. A "global"
project can serve as a shared resource pool.

## Architecture

Rust. DDD + Hexagonal. Four crates:

- **orchy-events** — Reusable event sourcing library. No domain dependencies.
- **orchy-core** — Domain types, traits, services. Zero external dependencies.
- **orchy-store-{memory,sqlite,pg}** — Store implementations.
- **orchy-server** — MCP server, HTTP transport, DI container.

Domain defines store traits. Infrastructure implements them. Services
orchestrate domain logic. MCP tools map HTTP requests to service calls.

All aggregates collect domain events via `EventCollector`. Store `save()`
methods drain events and persist them. Events are queryable for activity feeds.

## Key Patterns

**Store traits with `&mut` save** — `save(&mut entity)` to drain collected events.

**Constructor convention** — `Entity::new()` validates and creates (with events).
`Entity::restore(RestoreX { ... })` reconstructs from DB (no validation, no events).

**Value objects** — Immutable, validated. `ProjectId`, `Namespace`, `TaskId`, etc.
Never direct-cast. Always use constructors.

**Namespace hierarchy** — `/` (root), `/backend`, `/backend/auth`. Reads without
namespace see everything. Writes default to agent's current namespace.

**Task state machine** — `Pending -> Claimed -> InProgress -> Completed/Failed`.
Also `Blocked` (for dependencies), `Cancelled`. Parent tasks auto-complete.

**Optimistic concurrency** — Memory entries and documents use `Version` field.

**Resource locking** — TTL-based. Released on disconnect. Use for files, not data.

**Session continuity** — `save_context` before disconnect, `load_context` on
startup. Falls back to most recent snapshot in namespace if no own context exists.

## Agent Lifecycle

**Startup:**
1. `register_agent(project, description)` — roles auto-assigned from task demand
2. `get_project` + `get_project_summary` — understand project state
3. `list_skills(inherited: true)` — load conventions
4. `load_context` — find handoff notes from previous sessions
5. `search_memory` / `search_documents` — check existing knowledge
6. `check_mailbox` — read pending messages
7. `get_next_task` — claim work

**Working:**
- `heartbeat` every ~30s
- `poll_updates` + `check_mailbox` for reactivity
- `watch_task` to track dependencies
- `lock_resource` before editing shared files
- `write_memory` for decisions, `write_document` for analysis

**Completing:**
- `complete_task` with actionable summary (never just "done")
- `write_memory` for each key decision
- `write_document` for analysis/specs

**Disconnecting:**
- `save_context` with structured handoff: task ID, progress, blockers, decisions
- `disconnect` — tasks released to pending, locks freed, watchers removed

## Knowledge Capture

Knowledge must be externalized — agents don't retain state between sessions.

**Memory** (`write_memory`) — Short facts and decisions. Structured keys:
`decision/auth-algorithm`, `finding/db-pool-limit`, `pattern/error-handling`.
Searchable via `search_memory`.

**Documents** (`write_document`) — Long-form analysis, specs, architecture
decisions. Hierarchical paths: `specs/auth`, `architecture/database-design`.
Searchable via `search_documents`.

**Task notes** (`add_task_note`) — Progress notes on specific tasks. Persist
across agent sessions (not cleared on release).

**Context snapshots** (`save_context`) — Session handoff notes. Include current
task, progress, blockers, decisions.

**Skills** (`write_skill`) — Reusable conventions and patterns. Inherited through
namespace hierarchy. Agents should follow them.

A new agent joining the project should:
1. Read skills to understand conventions
2. Search memory/documents to understand decisions already made
3. Load context to find the latest handoff note
4. Check task notes for progress on specific work

## Maintenance Patterns

A "janitor" agent can compact and reorganize:

- **Compact memory** — `list_memory` -> merge related entries -> `write_memory` consolidated -> `delete_memory` old ones
- **Compact documents** — Merge overlapping docs into a single comprehensive one
- **Extract skills** — Read memory/documents for patterns, create skills from them
- **Reorganize tasks** — `merge_tasks` related items, `move_task` to correct namespace
- **Lock during compaction** — `lock_resource("compaction")` to prevent conflicts

## Decisions Log

- Memory uses optimistic concurrency (Version field), not locks
- EventLog trait replaced by io::Writer — stores implement Writer directly
- sea-query for dynamic SQL in sqlite/pg stores
- Restore structs over positional params for DB reconstruction
- UUID v7 everywhere for time-ordered IDs
- Embeddings provider lives in server crate, core only defines the trait
- TaskService uses 2 generics: `TS: TaskStore` + `S: AgentStore + WatcherStore + MessageStore + ReviewStore`
- poll_updates queries the events table, not task projections
- All tools require session except register_agent

## Code Style

- Rust edition 2024, DDD + Hexagonal
- No comments unless essential. No helper/utils files.
- Traits first in each file, then types, constructors, methods, getters
- `cargo fmt` before committing
- Conventional commits: `type(scope): description`
- No GPG signing, no Co-Authored-By, never push

## Running

```bash
cargo run -p orchy-server          # start MCP server (default: config.toml)
cargo test -p orchy-core           # domain tests
cargo test -p orchy-store-memory   # in-memory store tests
cargo test -p orchy-store-sqlite   # SQLite tests
```
