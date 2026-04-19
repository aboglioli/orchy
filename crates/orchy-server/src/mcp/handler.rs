use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

struct SessionState {
    agent_id: AgentId,
    org: OrganizationId,
    project: ProjectId,
    namespace: Namespace,
}

pub(crate) enum NamespacePolicy {
    Required,
    SessionDefault,
    RegisterIfNew,
}

#[derive(Clone)]
pub struct OrchyHandler {
    pub(crate) container: Arc<Container>,
    session: Arc<std::sync::RwLock<Option<SessionState>>>,
    mcp_session_id: Arc<std::sync::RwLock<Option<String>>>,
}

impl OrchyHandler {
    pub fn new(container: Arc<Container>) -> Self {
        Self {
            container,
            session: Arc::new(std::sync::RwLock::new(None)),
            mcp_session_id: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub(crate) fn set_mcp_session_id(&self, session_id: String) {
        if let Ok(mut guard) = self.mcp_session_id.write() {
            *guard = Some(session_id);
        }
    }

    pub(crate) fn get_session_agent(&self) -> Option<AgentId> {
        self.session
            .read()
            .ok()?
            .as_ref()
            .map(|s| s.agent_id.clone())
    }

    pub(crate) fn get_session_project(&self) -> Option<ProjectId> {
        self.session
            .read()
            .ok()?
            .as_ref()
            .map(|s| s.project.clone())
    }

    pub(crate) fn get_session_namespace(&self) -> Option<Namespace> {
        self.session
            .read()
            .ok()?
            .as_ref()
            .map(|s| s.namespace.clone())
    }

    pub(crate) fn require_session(
        &self,
    ) -> Result<(AgentId, OrganizationId, ProjectId, Namespace), String> {
        let guard = self
            .session
            .read()
            .map_err(|_| "session lock poisoned".to_string())?;
        match guard.as_ref() {
            Some(s) => Ok((
                s.agent_id.clone(),
                s.org.clone(),
                s.project.clone(),
                s.namespace.clone(),
            )),
            None => {
                Err("no agent registered for this session; call register_agent first".to_string())
            }
        }
    }

    pub(crate) async fn set_session(
        &self,
        agent_id: AgentId,
        org: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
    ) {
        if let Ok(mut guard) = self.session.write() {
            *guard = Some(SessionState {
                agent_id: agent_id.clone(),
                org,
                project,
                namespace,
            });
        }

        if let Some(session_id) = self
            .mcp_session_id
            .read()
            .ok()
            .and_then(|g| g.as_ref().cloned())
        {
            self.container
                .session_agents
                .write()
                .await
                .insert(session_id, agent_id);
        }
    }

    pub(crate) fn set_session_project_and_namespace(
        &self,
        project: ProjectId,
        namespace: Namespace,
    ) {
        if let Ok(mut guard) = self.session.write()
            && let Some(state) = guard.as_mut()
        {
            state.project = project;
            state.namespace = namespace;
        }
    }

    pub(crate) fn touch_heartbeat(&self) {
        if let Some(agent_id) = self.get_session_agent() {
            let container = self.container.clone();
            tokio::spawn(async move {
                let cmd = orchy_application::HeartbeatCommand {
                    agent_id: agent_id.to_string(),
                };
                let _ = container.app.heartbeat.execute(cmd).await;
            });
        }
    }

    pub(crate) async fn resolve_namespace(
        &self,
        param: Option<&str>,
        policy: NamespacePolicy,
    ) -> Result<Namespace, String> {
        self.resolve_namespace_for(param, policy, None, None).await
    }

    pub(crate) async fn resolve_namespace_for(
        &self,
        param: Option<&str>,
        policy: NamespacePolicy,
        explicit_org: Option<&OrganizationId>,
        explicit_project: Option<&ProjectId>,
    ) -> Result<Namespace, String> {
        let ns = match param {
            Some(s) if !s.is_empty() => {
                let normalized = if s.starts_with('/') {
                    s.to_string()
                } else {
                    format!("/{s}")
                };
                Namespace::try_from(normalized).map_err(|e| e.to_string())?
            }
            _ => match policy {
                NamespacePolicy::SessionDefault => {
                    self.get_session_namespace().unwrap_or_else(Namespace::root)
                }
                _ => Namespace::root(),
            },
        };

        if matches!(policy, NamespacePolicy::RegisterIfNew) {
            let org = match explicit_org {
                Some(o) => o.clone(),
                None => self
                    .session
                    .read()
                    .ok()
                    .and_then(|g| g.as_ref().map(|s| s.org.clone()))
                    .ok_or("no session org for namespace registration")?,
            };
            let project = match explicit_project {
                Some(p) => p.clone(),
                None => self
                    .get_session_project()
                    .ok_or("no session project for namespace registration")?,
            };

            use orchy_core::namespace::NamespaceStore;
            let _ = NamespaceStore::register(&*self.container.store, &org, &project, &ns).await;
        }

        Ok(ns)
    }

    pub(crate) async fn resolve_agent_id(&self, s: &str) -> Result<AgentId, String> {
        let agent_id = parse_agent_id(s)?;

        let project = self
            .get_session_project()
            .ok_or("no session project for agent lookup")?;

        let org = self
            .session
            .read()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.org.clone()))
            .unwrap_or_else(default_org);

        let agent = self
            .container
            .app
            .get_agent
            .execute(orchy_application::GetAgentCommand {
                agent_id: agent_id.to_string(),
            })
            .await
            .map_err(mcp_error)?;

        if agent.org_id != org.to_string() || agent.project != project.to_string() {
            return Err(format!("agent not found in current project: '{s}'"));
        }

        if agent.status == "disconnected" {
            return Err(format!("agent is disconnected: '{s}'"));
        }

        Ok(agent_id)
    }
}

pub(crate) fn default_org() -> OrganizationId {
    OrganizationId::new("default").unwrap()
}

pub(crate) fn parse_project(s: &str) -> Result<ProjectId, String> {
    ProjectId::try_from(s.to_string()).map_err(|e| e.to_string())
}

pub(crate) fn parse_agent_id(s: &str) -> Result<AgentId, String> {
    s.parse::<AgentId>()
        .map_err(|e| format!("invalid agent_id: {e}"))
}

pub(crate) fn to_json<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|e| format!("serialization error: {e}"))
}

pub(crate) fn mcp_error(e: orchy_core::error::Error) -> String {
    use orchy_core::error::Error;
    let (code, message) = match &e {
        Error::NotFound(_) => ("NOT_FOUND", e.to_string()),
        Error::InvalidInput(_) | Error::InvalidTransition { .. } | Error::DependencyNotMet(_) => {
            ("INVALID_INPUT", e.to_string())
        }
        Error::Conflict(_) | Error::VersionMismatch { .. } => ("CONFLICT", e.to_string()),
        Error::Embeddings(_) => ("EMBEDDINGS_ERROR", e.to_string()),
        Error::Store(_) => ("INTERNAL_ERROR", e.to_string()),
    };
    serde_json::json!({ "error": { "code": code, "message": message } }).to_string()
}

pub(crate) const INSTRUCTIONS: &str = "\
orchy — multi-agent coordination server.

You are part of a coordinated multi-agent system. orchy provides shared \
infrastructure: a task board, knowledge base, messaging, \
resource locks, and cross-project links. \
You bring the intelligence; orchy enforces the rules.

## On Session Start

1. `register_agent` — project, roles (optional), description. \
   Pass `id` to resume the same orchy agent after a **new MCP session** (orchy or client \
   restarted). Persist that id from the last registration or from `kind: \"context\"` handoff. \
   `session_status` explains reconnect if unsure. \
   `list_agents` accepts optional `project` before you register.
2. `get_agent_context` — returns project metadata, inbox messages, pending tasks \
   matching your roles, skills, and handoff context from previous sessions in one call.
3. `get_next_task` — `claim: true` (default) to claim; `claim: false` to peek only.
4. `heartbeat` — call every ~30s to stay alive.

## After orchy or MCP transport restart

MCP session ids are **in-memory** and do not survive a restart. The client must run a fresh MCP \
handshake; you may see **Session not found** until it does.

Your **orchy agent** (`id`) lives in the database — it is **not** auto-attached to a new MCP \
session. After reconnect, call **`register_agent` again with the same `id`** you used before \
(save it in the workspace or a handoff note). Tasks and knowledge remain under that id.

`register_agent`, `session_status`, `list_knowledge_types`, and `list_agents` (when \
`project` is passed) do not require a registered orchy session; most other tools do.

## Before Disconnecting

Always `write_knowledge(kind: \"context\", path: \"handoff\")` with a structured \
summary: current task, progress, blockers, decisions. This is the handoff note \
for the next agent.

## Namespaces

Resources live in namespaces: `/` (root), `/backend`, `/backend/auth`. \
Tools default to your current namespace when you omit the namespace parameter. \
Pass `namespace=/` to see all namespaces. \
Writes default to your current namespace. Namespaces are auto-created on first use.

## Task Workflow

pending → claimed → in_progress → completed/failed. \
Always claim before starting. If another agent claimed it, move on. \
`split_task` breaks a task into subtasks — parent auto-completes when all finish. \
`merge_tasks` consolidates related tasks. `delegate_task` creates subtasks \
without blocking the parent. Use `tag_task` / `untag_task` for labels. \
On disconnect, claimed tasks return to pending.

**Acceptance criteria:** Every task can have `acceptance_criteria` — a clear definition \
of done. Set on create (`post_task`) or update (`update_task`). \
Subtasks created via `split_task` each carry their own `acceptance_criteria`. \
Visible in `get_task` responses. Helps agents know exactly when work is complete.

**Task context in one call:** `get_task(task_id, include_dependencies=true, \
include_knowledge=true)` fetches a task with its blocking dependencies and linked \
knowledge entries together. Use this instead of separate fetches to avoid N+1 patterns.

## Coordination

- `write_knowledge` — persist decisions, discoveries, patterns. \
  Always `search_knowledge` first to avoid duplicating existing entries. \
  `search_knowledge` results include a `score` field (0.0–1.0 similarity). \
  Pass `min_score=0.75` to filter low-relevance results. \
  Call `list_knowledge_types` to see available types. \
  Optional `metadata` (JSON object string) merges; `metadata_remove` drops keys first. \
  `patch_knowledge_metadata` updates metadata only. \
  Use `change_knowledge_kind` to change an entry's kind (not via `write_knowledge` updates).
- `send_message` to coordinate (by agent ID, `role:name`, or `broadcast`).
- `lock_resource` before editing shared files to prevent conflicts.
- `poll_updates` + `check_mailbox` on each heartbeat cycle for reactivity.
- `write_knowledge(kind: \"context\")` before your session ends for continuity.
- Register without roles — orchy assigns them based on task demand.

## Knowledge Capture

You must externalize knowledge so future agents can benefit:

- After completing a task, `write_knowledge` for each key decision \
  (e.g. path: `auth-algorithm`, kind: `decision`).
- `complete_task` summary must be actionable: what was done, what was learned, \
  what the next agent should know. Never just 'done'.
- Before disconnecting, `write_knowledge(kind: \"context\", path: \"handoff\")` \
  with structured summary: current task, progress, blockers, decisions.
- When you discover something non-obvious (a gotcha, a pattern, a constraint), \
  write it to knowledge immediately — don't wait until task completion.
- Use `search_knowledge` before starting work to check \
  if a previous agent already explored this area.

## Graph (Relationships Between Resources)

Use edges to record meaningful relationships between tasks, knowledge entries, and agents. \
This builds a shared graph that any agent can traverse.

**Resource kinds for edges:** `task`, `knowledge`, `agent` only. Message resources cannot be graph nodes.

**When to create edges:**
- Task produces a knowledge artifact → `add_edge(from_kind=task, to_kind=knowledge, rel_type=produces)`
- Knowledge governs or informs a task → `add_edge(from_kind=knowledge, to_kind=task, rel_type=implements)`
- Task spawned subtasks (auto-created by `split_task` and `delegate_task`)
- Knowledge entry supersedes an old one → `add_edge(rel_type=supersedes)`
- Knowledge summarizes another → `add_edge(rel_type=summarizes)`
- Two entries are conceptually linked → `add_edge(rel_type=related_to)`
- Task was derived from another → `add_edge(rel_type=derived_from)`
- `write_knowledge` with `task_id` auto-creates a `produces` edge (task → knowledge)
- `merge_tasks` auto-creates `merged_from` edges (merged ← each source)

**Relationship types (9):**
- `derived_from` — this was created/informed by that
- `produces` — completing this produced that as output
- `supersedes` — this replaces/obsoletes that; auto-created by replace_task
- `merged_from` — N-to-1 consolidation (merged task ← source tasks); auto-created by merge_tasks
- `summarizes` — 1-to-1 distillation of another entry
- `implements` — this executes/fulfills that (task implementing a plan)
- `spawns` — this triggered/created that; auto-created by split_task, delegate_task, and post_task (when parent_id is set)
- `depends_on` — task A depends on task B completing first; auto-created by add_dependency, post_task (with depends_on list), auto-deleted by remove_dependency
- `related_to` — general symmetric peer relationship

**Deduplication:** `add_edge` returns an error if the same (from, rel_type, to) triple already exists.

**Query patterns:**
- `get_neighbors(kind=task, id=..., direction=outgoing)` — direct connections (1 hop)
- `get_neighbors(..., include_nodes=true)` — edges plus NodeSummary (title, status, priority, \
  tags, content snippet) for each connected resource; use node_content_limit to cap length
- `get_graph(kind=task, id=root, max_depth=3)` — traverse task tree and connected knowledge
- `get_graph(..., include_nodes=true)` — edges plus NodeSummary (title, content, tags, status, \
  priority, updated_at) for every touched resource in one call
- `get_graph(kind=knowledge, id=..., rel_types=[supersedes], direction=incoming)` — find what supersedes this entry
- `list_edges(rel_type=spawns)` — browse all edges in the org without a known root

Edges carry `source_kind` and `source_id` provenance fields identifying which tool or \
agent created them. Edges are org-scoped and directed. `get_neighbors` returns direct \
connections; `get_graph` recurses up to max_depth hops (default 3, max 10).";
