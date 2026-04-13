use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::namespace::{Namespace, ProjectId};

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
