# orchy

Multi-agent coordination server. Orchy is the shared infrastructure that
allows multiple AI agents (Claude Code, Codex, Gemini, Cursor, etc.) to work
together on complex goals — like a company operating system for agents.

## What Orchy Does

Orchy exposes ~80 MCP tools over Streamable HTTP. Agents connect, register,
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
finding must be externalized or it's lost. Three layers:

- **Memory** — key-value facts and decisions. Short, searchable. Use structured
  keys: `decision/auth-algorithm`, `finding/db-pool-limit`, `pattern/error-handling`.
  Think of it as the organization's environment variables.
- **Documents** — long-form markdown. Specs, architecture decisions, analysis,
  post-mortems. Hierarchical paths: `specs/auth`, `architecture/database`.
  Think of it as the organization's wiki.
- **Skills** — reusable conventions and instructions that all agents follow.
  Inherited through namespace hierarchy. Think of it as the organization's
  playbook/runbook.
- **Contexts** — session handoff snapshots. What you were working on, what you
  accomplished, what's left. The next agent loads this to continue your work.
- **Cross-project sharing** — link projects to import skills and memory. A
  "global" project serves as a shared resource pool across all projects.

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
