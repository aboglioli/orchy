use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::message::MessageId;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{ReviewId, TaskId};

use crate::container::Container;

struct SessionState {
    agent_id: AgentId,
    project: ProjectId,
    namespace: Namespace,
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
        self.session.read().ok()?.as_ref().map(|s| s.agent_id)
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

    pub(crate) fn require_session(&self) -> Result<(AgentId, ProjectId, Namespace), String> {
        let guard = self
            .session
            .read()
            .map_err(|_| "session lock poisoned".to_string())?;
        match guard.as_ref() {
            Some(s) => Ok((s.agent_id, s.project.clone(), s.namespace.clone())),
            None => {
                Err("no agent registered for this session; call register_agent first".to_string())
            }
        }
    }

    pub(crate) fn set_session(&self, agent_id: AgentId, project: ProjectId, namespace: Namespace) {
        if let Ok(mut guard) = self.session.write() {
            *guard = Some(SessionState {
                agent_id,
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
            let session_agents = self.container.session_agents.clone();
            tokio::spawn(async move {
                session_agents.write().await.insert(session_id, agent_id);
            });
        }
    }

    pub(crate) fn set_session_namespace(&self, namespace: Namespace) {
        if let Ok(mut guard) = self.session.write() {
            if let Some(state) = guard.as_mut() {
                state.namespace = namespace;
            }
        }
    }

    pub(crate) fn touch_heartbeat(&self) {
        if let Some(agent_id) = self.get_session_agent() {
            let container = self.container.clone();
            tokio::spawn(async move {
                let _ = container.agent_service.heartbeat(&agent_id).await;
            });
        }
    }

    pub(crate) fn build_namespace(&self, scope: Option<&str>) -> Result<Namespace, String> {
        let _ = self
            .get_session_project()
            .ok_or("no agent registered for this session; call register_agent first")?;

        match scope {
            Some(s) if !s.is_empty() => {
                Namespace::try_from(format!("/{s}")).map_err(|e| e.to_string())
            }
            _ => Ok(Namespace::root()),
        }
    }

    pub(crate) async fn build_and_register_namespace(
        &self,
        scope: Option<&str>,
    ) -> Result<Namespace, String> {
        let ns = self.build_namespace(scope)?;
        if let Some(project) = self.get_session_project() {
            use orchy_core::namespace::NamespaceStore;
            let default_org = OrganizationId::new("default").unwrap();
            let _ =
                NamespaceStore::register(&*self.container.store, &default_org, &project, &ns).await;
        }
        Ok(ns)
    }

    pub(crate) async fn resolve_agent_id(&self, s: &str) -> Result<AgentId, String> {
        if let Ok(id) = s.parse::<AgentId>() {
            return Ok(id);
        }

        let project = self
            .get_session_project()
            .ok_or("no session project for alias resolution")?;

        let alias = orchy_core::agent::Alias::new(s).map_err(|e| e.to_string())?;

        let default_org = OrganizationId::new("default").unwrap();
        match self
            .container
            .agent_service
            .find_by_alias(&default_org, &project, &alias)
            .await
        {
            Ok(Some(agent)) => Ok(agent.id()),
            Ok(None) => Err(format!(
                "agent not found: '{s}' (not a UUID or known alias)"
            )),
            Err(e) => Err(e.to_string()),
        }
    }

    pub(crate) fn build_optional_namespace(
        &self,
        scope: Option<&str>,
    ) -> Result<Option<Namespace>, String> {
        match scope {
            Some(_) => self.build_namespace(scope).map(Some),
            None => Ok(None),
        }
    }
}

pub(crate) fn parse_project(s: &str) -> Result<ProjectId, String> {
    ProjectId::try_from(s.to_string()).map_err(|e| e.to_string())
}

pub(crate) fn parse_namespace(s: &str) -> Result<Namespace, String> {
    Namespace::try_from(s.to_string()).map_err(|e| e.to_string())
}

pub(crate) fn parse_task_id(s: &str) -> Result<TaskId, String> {
    s.parse::<TaskId>()
        .map_err(|e| format!("invalid task_id: {e}"))
}

pub(crate) fn parse_review_id(s: &str) -> Result<ReviewId, String> {
    s.parse::<ReviewId>()
        .map_err(|e| format!("invalid review_id: {e}"))
}

pub(crate) fn parse_agent_id(s: &str) -> Result<AgentId, String> {
    s.parse::<AgentId>()
        .map_err(|e| format!("invalid agent_id: {e}"))
}

pub(crate) fn parse_message_id(s: &str) -> Result<MessageId, String> {
    s.parse::<MessageId>()
        .map_err(|e| format!("invalid message_id: {e}"))
}

pub(crate) fn to_json<T: serde::Serialize>(val: &T) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|e| format!("serialization error: {e}"))
}

pub(crate) const INSTRUCTIONS: &str = "\
orchy — multi-agent coordination server.

You are part of a coordinated multi-agent system. orchy provides shared \
infrastructure: a task board, knowledge base, messaging, \
resource locks, and cross-project links. \
You bring the intelligence; orchy enforces the rules.

## On Session Start

1. `register_agent` — project, roles (optional), description. \
   Pass `agent_id` to resume the same orchy agent after a **new MCP session** (orchy or client \
   restarted). Persist that UUID from the last registration or from `kind: \"context\"` handoff. \
   `session_status` explains reconnect if unsure. \
   `list_agents` accepts optional `project` before you register.
2. `get_project` — metadata; set `include_summary: true` for task/agent overview.
3. `list_knowledge(kind: \"skill\")` — load conventions; `kind: \"overview\"` for bootstrap summaries. Follow skills.
4. `list_knowledge(kind: \"context\")` — check for handoff notes from previous agents. \
   Also `search_knowledge` to find relevant decisions and discoveries.
5. `check_mailbox` — read incoming messages. `check_sent_messages` for sent mail.
6. `get_next_task` — `claim: true` (default) to claim; `claim: false` to peek only.
7. `heartbeat` — call every ~30s to stay alive.

## After orchy or MCP transport restart

MCP session ids are **in-memory** and do not survive a restart. The client must run a fresh MCP \
handshake; you may see **Session not found** until it does.

Your **orchy agent** (`agent_id`) lives in the database — it is **not** auto-attached to a new MCP \
session. After reconnect, call **`register_agent` again with the same `agent_id`** you used before \
(save it in the workspace or a handoff note). Tasks and knowledge remain under that id.

`register_agent`, `session_status`, `mark_read`, `list_conversation`, and `list_agents` (when \
`project` is passed) do not require a registered orchy session; most other tools do.

## Before Disconnecting

Always `write_knowledge(kind: \"context\", path: \"handoff\")` with a structured \
summary: current task, progress, blockers, decisions. This is the handoff note \
for the next agent.

## Namespaces

Resources live in namespaces: `/` (root), `/backend`, `/backend/auth`. \
Omit namespace on reads to see everything. Writes default to your current \
namespace. Namespaces are auto-created on first use.

## Task Workflow

pending → claimed → in_progress → completed/failed. \
Always claim before starting. If another agent claimed it, move on. \
`split_task` breaks a task into subtasks — parent auto-completes when all finish. \
`merge_tasks` consolidates related tasks. `delegate_task` creates subtasks \
without blocking the parent. Use `tag_task` / `untag_task` for labels. \
On disconnect, claimed tasks return to pending.

## Coordination

- `write_knowledge` — persist decisions, discoveries, patterns. \
  Always `search_knowledge` first to avoid duplicating existing entries. \
  Call `list_knowledge_types` to see available types. \
  Optional `metadata` (JSON object string) merges; `metadata_remove` drops keys first. \
  `patch_knowledge_metadata` updates metadata only. \
  Use `change_knowledge_kind` to change an entry's kind (not via `write_knowledge` updates).
- `send_message` to coordinate (by agent ID, `role:name`, or `broadcast`).
- `lock_resource` before editing shared files to prevent conflicts.
- `watch_task` to get notified when a task status changes.
- `request_review` to ask another agent to review your work.
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
  if a previous agent already explored this area.";
