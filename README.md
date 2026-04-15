# orchy

Multi-agent coordination server. Shared infrastructure for AI agents: task
board, unified knowledge base, messaging, resource locking, and project
context — exposed as **65** MCP tools over Streamable HTTP.

orchy is not an orchestrator. Agents bring the intelligence; orchy provides
the coordination layer and enforces the rules.

## Quick start

```bash
cargo run -p orchy-server
```

MCP server at `http://127.0.0.1:3100/mcp`. Bootstrap prompt at
`http://127.0.0.1:3100/bootstrap/<project>`.

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

### Knowledge

All persistent knowledge lives in a unified system with typed entries.
Each entry has a `kind`, `path`, `title`, `content`, `tags`, and `version`.

| Kind | Description |
|------|-------------|
| `note` | General observation or record |
| `decision` | A choice made with rationale |
| `discovery` | Something found or learned |
| `pattern` | A recurring approach or convention |
| `context` | Session summary / agent state snapshot |
| `document` | Long-form structured content |
| `config` | Configuration or setup information |
| `reference` | External reference or link |
| `plan` | Strategy, roadmap, or approach |
| `log` | Activity or change log entry |
| `skill` | Instruction or convention agents must follow |

Paths are hierarchical: `db-choice`, `auth/jwt-strategy`, `api-design`.
Skills (kind=skill) inherit through namespace hierarchy.

### Tasks

```
Pending → Claimed → InProgress → Completed/Failed/Cancelled
```

Tasks support hierarchy (`split_task`), dependencies, tags, watchers,
and reviews. Parent tasks auto-complete when all subtasks finish.

### Agent lifecycle

1. Register with `register_agent` (roles auto-assigned if omitted)
2. `heartbeat` every ~30s; after registration, MCP tool invocations refresh liveness
3. On disconnect: tasks released, locks freed, watchers removed

### Resource locking

TTL-based locking for any named resource. Auto-expires and cleaned up
on agent disconnect.

### Project links

Projects can link to other projects to share knowledge entries.

### Event log

Every state change is recorded as a semantic domain event. Query with
`poll_updates`.

## MCP Tools

Authoritative definitions: `crates/orchy-server/src/mcp/tools.rs` and
`crates/orchy-server/src/mcp/params.rs`. A running server exposes the current
set via MCP `list_tools`.

**Session** — `yes` means call `register_agent` first for that MCP session.
**no** — callable without registration. **partial** — `list_agents` only: pass
`project`, or register to use the session project.

Tools without registration: `register_agent`, `list_knowledge_types`,
`mark_read`, `list_conversation`, and `list_agents` when `project` is set.

---

### Agent

| Tool | Session | Parameters |
|------|---------|-----------|
| `register_agent` | no | `project` (req), `description` (req), `namespace`, `roles`, `agent_id`, `parent_id`, `metadata` |
| `list_agents` | partial | `project` |
| `change_roles` | yes | `roles` (req) |
| `move_agent` | yes | `namespace` (req) |
| `heartbeat` | yes | |
| `disconnect` | yes | |

### Tasks

| Tool | Session | Parameters |
|------|---------|-----------|
| `post_task` | yes | `title` (req), `description` (req), `namespace`, `parent_id`, `priority`, `assigned_roles`, `depends_on` |
| `get_task` | yes | `task_id` (req) |
| `get_next_task` | yes | `namespace`, `role`, `claim` |
| `list_tasks` | yes | `namespace`, `status`, `parent_id` |
| `claim_task` | yes | `task_id` (req), `start` |
| `start_task` | yes | `task_id` (req) |
| `complete_task` | yes | `task_id` (req), `summary` |
| `fail_task` | yes | `task_id` (req), `reason` |
| `cancel_task` | yes | `task_id` (req), `reason` |
| `update_task` | yes | `task_id` (req), `title`, `description`, `priority` |
| `unblock_task` | yes | `task_id` (req) |
| `release_task` | yes | `task_id` (req) |
| `assign_task` | yes | `task_id` (req), `agent_id` (req) |
| `delegate_task` | yes | `task_id` (req), `title` (req), `description` (req), `priority`, `assigned_roles` |
| `add_task_note` | yes | `task_id` (req), `body` (req) |
| `split_task` | yes | `task_id` (req), `subtasks` (req) |
| `replace_task` | yes | `task_id` (req), `replacements` (req), `reason` |
| `merge_tasks` | yes | `task_ids` (req), `title` (req), `description` (req) |
| `add_dependency` | yes | `task_id` (req), `dependency_id` (req) |
| `remove_dependency` | yes | `task_id` (req), `dependency_id` (req) |
| `tag_task` | yes | `task_id` (req), `tag` (req) |
| `untag_task` | yes | `task_id` (req), `tag` (req) |
| `list_tags` | yes | `namespace` |
| `move_task` | yes | `task_id` (req), `new_namespace` (req) |
| `watch_task` | yes | `task_id` (req) |
| `unwatch_task` | yes | `task_id` (req) |
| `request_review` | yes | `task_id` (req), `reviewer_agent`, `reviewer_role` |
| `resolve_review` | yes | `review_id` (req), `approved` (req), `comments` |
| `list_reviews` | yes | `task_id` (req) |
| `get_review` | yes | `review_id` (req) |

### Knowledge

| Tool | Session | Parameters |
|------|---------|-----------|
| `list_knowledge_types` | no | |
| `write_knowledge` | yes | `path` (req), `kind` (req), `title` (req), `content` (req), `namespace`, `tags`, `version`, `metadata` |
| `read_knowledge` | yes | `path` (req), `namespace` |
| `list_knowledge` | yes | `namespace`, `kind`, `tag`, `path_prefix`, `agent_id` |
| `search_knowledge` | yes | `query` (req), `namespace`, `kind`, `limit` |
| `delete_knowledge` | yes | `path` (req), `namespace` |
| `append_knowledge` | yes | `path` (req), `kind` (req), `value` (req), `namespace`, `separator` |
| `move_knowledge` | yes | `path` (req), `new_namespace` (req), `namespace` |
| `rename_knowledge` | yes | `path` (req), `new_path` (req), `namespace` |
| `tag_knowledge` | yes | `path` (req), `tag` (req), `namespace` |
| `untag_knowledge` | yes | `path` (req), `tag` (req), `namespace` |
| `import_knowledge` | yes | `source_project` (req), `path` (req), `source_namespace` |

### Messages

| Tool | Session | Parameters |
|------|---------|-----------|
| `send_message` | yes | `to` (req), `body` (req), `namespace`, `reply_to` |
| `check_mailbox` | yes | `namespace` |
| `check_sent_messages` | yes | `namespace` |
| `mark_read` | no | `message_ids` (req) |
| `list_conversation` | no | `message_id` (req), `limit` |

### Resource locking

| Tool | Session | Parameters |
|------|---------|-----------|
| `lock_resource` | yes | `name` (req), `namespace`, `ttl_secs` |
| `unlock_resource` | yes | `name` (req), `namespace` |
| `check_lock` | yes | `name` (req), `namespace` |

### Project

| Tool | Session | Parameters |
|------|---------|-----------|
| `get_project` | yes | `include_summary` |
| `update_project` | yes | `description` (req) |
| `add_project_note` | yes | `body` (req) |
| `get_project_overview` | yes | `namespace` |
| `list_namespaces` | yes | |
| `poll_updates` | yes | `since`, `limit` |

### Project links

| Tool | Session | Parameters |
|------|---------|-----------|
| `link_project` | yes | `source_project` (req), `resource_types` (req) |
| `unlink_project` | yes | `source_project` (req) |
| `list_project_links` | yes | |

## License

MIT
