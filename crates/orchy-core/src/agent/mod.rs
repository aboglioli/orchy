pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::Result;
use crate::namespace::{Namespace, ProjectId};

pub trait AgentStore: Send + Sync {
    fn save(&self, agent: &Agent) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &AgentId) -> impl Future<Output = Result<Option<Agent>>> + Send;
    fn list(&self) -> impl Future<Output = Result<Vec<Agent>>> + Send;
    fn find_timed_out(&self, timeout_secs: u64) -> impl Future<Output = Result<Vec<Agent>>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for AgentId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    #[default]
    Online,
    Busy,
    Idle,
    Disconnected,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AgentStatus::Online => "online",
            AgentStatus::Busy => "busy",
            AgentStatus::Idle => "idle",
            AgentStatus::Disconnected => "disconnected",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    id: AgentId,
    project: ProjectId,
    namespace: Namespace,
    parent_id: Option<AgentId>,
    roles: Vec<String>,
    description: String,
    status: AgentStatus,
    last_heartbeat: DateTime<Utc>,
    connected_at: DateTime<Utc>,
    metadata: HashMap<String, String>,
}

impl Agent {
    pub fn register(
        project: ProjectId,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
        metadata: HashMap<String, String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: AgentId::new(),
            project,
            namespace,
            parent_id: None,
            roles,
            description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata,
        }
    }

    pub fn from_parent(
        parent: &Agent,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: AgentId::new(),
            project: parent.project.clone(),
            namespace,
            parent_id: Some(parent.id),
            roles,
            description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata: parent.metadata.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: AgentId,
        project: ProjectId,
        namespace: Namespace,
        parent_id: Option<AgentId>,
        roles: Vec<String>,
        description: String,
        status: AgentStatus,
        last_heartbeat: DateTime<Utc>,
        connected_at: DateTime<Utc>,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            id,
            project,
            namespace,
            parent_id,
            roles,
            description,
            status,
            last_heartbeat,
            connected_at,
            metadata,
        }
    }

    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
        if self.status == AgentStatus::Disconnected {
            self.status = AgentStatus::Online;
        }
    }

    pub fn disconnect(&mut self) {
        self.status = AgentStatus::Disconnected;
    }

    pub fn update_status(&mut self, status: AgentStatus) {
        self.status = status;
    }

    pub fn update_roles(&mut self, roles: Vec<String>) {
        self.roles = roles;
    }

    pub fn is_timed_out(&self, timeout_secs: u64) -> bool {
        self.status != AgentStatus::Disconnected
            && (Utc::now() - self.last_heartbeat) > chrono::Duration::seconds(timeout_secs as i64)
    }

    pub fn id(&self) -> AgentId {
        self.id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn parent_id(&self) -> Option<AgentId> {
        self.parent_id
    }
    pub fn roles(&self) -> &[String] {
        &self.roles
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn status(&self) -> AgentStatus {
        self.status
    }
    pub fn last_heartbeat(&self) -> DateTime<Utc> {
        self.last_heartbeat
    }
    pub fn connected_at(&self) -> DateTime<Utc> {
        self.connected_at
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
}

#[derive(Debug, Clone)]
pub struct RegisterAgent {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub roles: Vec<String>,
    pub description: String,
    pub parent_id: Option<AgentId>,
    pub metadata: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    fn test_namespace() -> Namespace {
        Namespace::try_from("test").unwrap()
    }

    fn make_agent() -> Agent {
        Agent::register(
            test_project(),
            test_namespace(),
            vec!["coder".to_string()],
            "test agent".to_string(),
            HashMap::new(),
        )
    }

    #[test]
    fn register_creates_online_agent() {
        let agent = make_agent();
        assert_eq!(agent.status(), AgentStatus::Online);
        assert_eq!(agent.roles(), &["coder"]);
        assert!(agent.parent_id().is_none());
    }

    #[test]
    fn from_parent_inherits_project_and_sets_parent() {
        let parent = make_agent();
        let child = Agent::from_parent(
            &parent,
            test_namespace(),
            vec!["reviewer".to_string()],
            "child agent".to_string(),
        );
        assert_eq!(child.project(), parent.project());
        assert_eq!(child.parent_id(), Some(parent.id()));
        assert_eq!(child.roles(), &["reviewer"]);
        assert_eq!(child.status(), AgentStatus::Online);
    }

    #[test]
    fn heartbeat_updates_timestamp() {
        let mut agent = make_agent();
        let before = agent.last_heartbeat();
        sleep(Duration::from_millis(10));
        agent.heartbeat();
        assert!(agent.last_heartbeat() > before);
    }

    #[test]
    fn heartbeat_reconnects_disconnected() {
        let mut agent = make_agent();
        agent.disconnect();
        agent.heartbeat();
        assert_eq!(agent.status(), AgentStatus::Online);
    }

    #[test]
    fn disconnect_sets_status() {
        let mut agent = make_agent();
        agent.disconnect();
        assert_eq!(agent.status(), AgentStatus::Disconnected);
    }

    #[test]
    fn is_timed_out_when_stale() {
        let mut agent = make_agent();
        agent.heartbeat();
        sleep(Duration::from_millis(10));
        assert!(agent.is_timed_out(0));
    }

    #[test]
    fn is_timed_out_false_when_disconnected() {
        let mut agent = make_agent();
        agent.disconnect();
        sleep(Duration::from_millis(10));
        assert!(!agent.is_timed_out(0));
    }
}
