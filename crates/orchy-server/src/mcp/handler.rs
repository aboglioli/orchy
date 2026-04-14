use std::sync::Arc;

use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, PaginatedRequestParams, Prompt, PromptMessage,
    PromptMessageRole, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};

use orchy_core::agent::AgentId;
use orchy_core::message::MessageId;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::task::TaskId;

use crate::container::Container;

type ListPromptsResult = rmcp::model::ListPromptsResult;

struct SessionState {
    agent_id: AgentId,
    project: ProjectId,
    namespace: Namespace,
}

#[derive(Clone)]
pub struct OrchyHandler {
    pub(crate) container: Arc<Container>,
    session: Arc<std::sync::RwLock<Option<SessionState>>>,
}

impl OrchyHandler {
    pub fn new(container: Arc<Container>) -> Self {
        Self {
            container,
            session: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub(crate) fn get_session_agent(&self) -> Option<AgentId> {
        self.session.read().unwrap().as_ref().map(|s| s.agent_id)
    }

    pub(crate) fn get_session_project(&self) -> Option<ProjectId> {
        self.session
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.project.clone())
    }

    pub(crate) fn get_session_namespace(&self) -> Option<Namespace> {
        self.session
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.namespace.clone())
    }

    pub(crate) fn require_session(&self) -> Result<(AgentId, ProjectId, Namespace), String> {
        let guard = self.session.read().unwrap();
        match guard.as_ref() {
            Some(s) => Ok((s.agent_id, s.project.clone(), s.namespace.clone())),
            None => {
                Err("no agent registered for this session; call register_agent first".to_string())
            }
        }
    }

    pub(crate) fn set_session(&self, agent_id: AgentId, project: ProjectId, namespace: Namespace) {
        *self.session.write().unwrap() = Some(SessionState {
            agent_id,
            project,
            namespace,
        });
    }

    pub(crate) fn set_session_namespace(&self, namespace: Namespace) {
        if let Some(state) = self.session.write().unwrap().as_mut() {
            state.namespace = namespace;
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
            let _ = NamespaceStore::register(&*self.container.store, &project, &ns).await;
        }
        Ok(ns)
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

const INSTRUCTIONS: &str = "\
orchy — multi-agent coordination server.

You are part of a coordinated multi-agent system. orchy provides shared \
infrastructure: a task board, knowledge base, messaging, \
resource locks, and cross-project links. \
You bring the intelligence; orchy enforces the rules.

## On Session Start

1. `register_agent` — project, roles (optional), description. \
   Pass `agent_id` to resume a previous session.
2. `get_project` + `get_project_summary` — load project context.
3. `list_knowledge(kind: \"skill\")` — load conventions. Follow them.
4. `list_knowledge(kind: \"context\")` — check for handoff notes from previous agents. \
   Also `search_knowledge` to find relevant decisions and discoveries.
5. `check_mailbox` — check for messages from other agents.
6. `get_next_task` — claim work. Tasks released by a disconnected agent \
   return to pending and can be re-claimed.
7. `heartbeat` — call every ~30s to stay alive.

## Before Disconnecting

Always `write_knowledge(kind: \"context\", path: \"context/handoff\")` with a structured \
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
without blocking the parent. Use `tag_task` for cross-cutting labels. \
On disconnect, claimed tasks return to pending.

## Coordination

- `write_knowledge` — persist decisions, discoveries, patterns, configs, plans. \
  Call `list_knowledge_types` to see available types.
- `send_message` to coordinate (by agent ID, `role:name`, or `broadcast`).
- `lock_resource` before editing shared files to prevent conflicts.
- `watch_task` to get notified when a task status changes.
- `request_review` to ask another agent to review your work.
- `poll_updates` + `check_mailbox` on each heartbeat cycle for reactivity.
- `write_knowledge(kind: \"context\")` before your session ends for continuity.
- `link_project` + `import_knowledge` to share knowledge across projects.
- Register without roles — orchy assigns them based on task demand.

## Knowledge Capture

You must externalize knowledge so future agents can benefit:

- After completing a task, `write_knowledge` for each key decision \
  (e.g. path: `decisions/auth-algorithm`, type: `decision`).
- `complete_task` summary must be actionable: what was done, what was learned, \
  what the next agent should know. Never just 'done'.
- Before disconnecting, `write_knowledge(kind: \"context\", path: \"context/handoff\")` \
  with structured summary: current task, progress, blockers, decisions.
- When you discover something non-obvious (a gotcha, a pattern, a constraint), \
  write it to knowledge immediately — don't wait until task completion.
- Use `search_knowledge` before starting work to check \
  if a previous agent already explored this area.";

impl ServerHandler for OrchyHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_instructions(INSTRUCTIONS.to_string())
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, ErrorData> {
        self.touch_heartbeat();
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        Self::tool_router().call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: Self::tool_router().list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let (project, namespace) = match (self.get_session_project(), self.get_session_namespace())
        {
            (Some(p), Some(ns)) => (p, ns),
            _ => {
                return Ok(ListPromptsResult {
                    prompts: vec![],
                    meta: None,
                    next_cursor: None,
                });
            }
        };

        let skills = self
            .container
            .knowledge_service
            .list_skills(&project, &namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let prompts = skills
            .into_iter()
            .map(|s| {
                Prompt::new(
                    s.title().to_string(),
                    Some(s.title().to_string()),
                    None,
                )
            })
            .collect();

        Ok(ListPromptsResult {
            prompts,
            meta: None,
            next_cursor: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        let project = self
            .get_session_project()
            .ok_or_else(|| ErrorData::internal_error("no session project", None))?;
        let namespace = self
            .get_session_namespace()
            .ok_or_else(|| ErrorData::internal_error("no session namespace", None))?;

        let skills = self
            .container
            .knowledge_service
            .list_skills(&project, &namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let entry = skills
            .into_iter()
            .find(|s| s.title() == request.name)
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    format!("skill '{}' not found", request.name),
                    None,
                )
            })?;

        let mut result = GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            entry.content().to_string(),
        )]);
        result.description = Some(entry.title().to_string());
        Ok(result)
    }
}
