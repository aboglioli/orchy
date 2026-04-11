use std::sync::Arc;

use orchy_core::value_objects::AgentId;

use crate::container::Container;

#[derive(Clone)]
pub struct OrchyHandler {
    pub(crate) container: Arc<Container>,
    pub(crate) session_agent: Arc<std::sync::RwLock<Option<AgentId>>>,
}

impl OrchyHandler {
    pub fn new(container: Arc<Container>) -> Self {
        Self {
            container,
            session_agent: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    pub(crate) fn get_session_agent(&self) -> Option<AgentId> {
        self.session_agent.read().unwrap().clone()
    }

    pub(crate) fn require_session_agent(&self) -> Result<AgentId, String> {
        self.get_session_agent()
            .ok_or_else(|| "no agent registered for this session; call register_agent first".to_string())
    }

    pub(crate) fn set_session_agent(&self, id: AgentId) {
        *self.session_agent.write().unwrap() = Some(id);
    }
}
