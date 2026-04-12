use std::sync::Arc;

use orchy_core::value_objects::{AgentId, Namespace};

use crate::container::Container;

struct SessionState {
    agent_id: AgentId,
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

    pub(crate) fn get_session_namespace(&self) -> Option<Namespace> {
        self.session
            .read()
            .unwrap()
            .as_ref()
            .map(|s| s.namespace.clone())
    }

    pub(crate) fn require_session(&self) -> Result<(AgentId, Namespace), String> {
        let guard = self.session.read().unwrap();
        match guard.as_ref() {
            Some(s) => Ok((s.agent_id, s.namespace.clone())),
            None => {
                Err("no agent registered for this session; call register_agent first".to_string())
            }
        }
    }

    pub(crate) fn set_session(&self, agent_id: AgentId, namespace: Namespace) {
        *self.session.write().unwrap() = Some(SessionState {
            agent_id,
            namespace,
        });
    }

    pub(crate) fn touch_heartbeat(&self) {
        if let Some(agent_id) = self.get_session_agent() {
            let container = self.container.clone();
            tokio::spawn(async move {
                let _ = container.agent_service.heartbeat(&agent_id).await;
            });
        }
    }

    pub(crate) fn resolve_namespace(&self, explicit: Option<&str>) -> Result<Namespace, String> {
        match explicit {
            Some(ns_str) => {
                let ns = parse_namespace(ns_str)?;
                let session_ns = self
                    .get_session_namespace()
                    .ok_or("no agent registered for this session; call register_agent first")?;
                if ns.project() != session_ns.project() {
                    return Err(format!(
                        "namespace '{}' does not belong to session project '{}'; \
                         the first segment must match the project you registered with",
                        ns,
                        session_ns.project()
                    ));
                }
                Ok(ns)
            }
            None => self.get_session_namespace().ok_or_else(|| {
                "no agent registered for this session; call register_agent first".to_string()
            }),
        }
    }
}

pub(crate) fn parse_namespace(s: &str) -> Result<Namespace, String> {
    Namespace::try_from(s.to_string())
}
