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
infrastructure: a task board, shared memory, messaging, skills, and \
project context. You bring the intelligence; orchy enforces the rules.

## On Session Start

1. Call `register_agent` with your project, roles, and description.
2. Call `list_skills(inherited=true)` and follow the project conventions.
3. Call `get_project` to read the project description and notes.
4. Call `get_next_task` to claim work, or `check_mailbox` for messages.
5. Call `heartbeat` every ~30s to signal liveness.

## Project & Namespace

Each agent belongs to a project (e.g. `my-project`). Resources are \
organized in namespaces within the project: `/` is root, `/backend`, \
`/backend/auth` are scopes. Namespace is optional for reading — omit \
it to see all project resources. Write operations default to your \
current namespace. Use `move_agent` to switch namespaces. Use \
`list_namespaces` to discover available scopes.

## Coordination

- Claim tasks before working. Complete them with a summary when done.
- Split large tasks with `split_task` — parent auto-completes when subtasks finish.
- Replace tasks with `replace_task` to cancel and create new ones.
- Manage dependencies with `add_dependency` and `remove_dependency`.
- Use shared memory to store decisions and context for other agents.
- Use messages to coordinate with teammates. Reply with `reply_to`.
- Check delivery status with `check_sent_messages`.
- Browse conversation threads with `list_conversation`.
- Save context before your session ends for continuity.
- Use `list_skills(inherited=true)` to get project conventions.
- Register without roles to let orchy assign roles based on task demand.

## Bootstrap Prompt

If your client doesn't support MCP instructions, call `get_bootstrap_prompt` \
to get a full copy-pasteable prompt with all orchy instructions and project skills.";

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
            .skill_service
            .list_with_inherited(&project, &namespace)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let prompts = skills
            .into_iter()
            .map(|s| {
                Prompt::new(
                    s.name().to_string(),
                    Some(s.description().to_string()),
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

        let skill = self
            .container
            .skill_service
            .read(&project, &namespace, &request.name)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let skill = match skill {
            Some(s) => s,
            None => {
                let inherited = self
                    .container
                    .skill_service
                    .list_with_inherited(&project, &namespace)
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

                inherited
                    .into_iter()
                    .find(|s| s.name() == request.name)
                    .ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("skill '{}' not found", request.name),
                            None,
                        )
                    })?
            }
        };

        let mut result = GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            skill.content().to_string(),
        )]);
        result.description = Some(skill.description().to_string());
        Ok(result)
    }
}
