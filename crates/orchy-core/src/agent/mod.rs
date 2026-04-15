pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

use self::events as agent_events;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Alias(String);

impl Alias {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.is_empty() {
            return Err(Error::InvalidInput("alias must not be empty".into()));
        }
        for ch in s.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                return Err(Error::InvalidInput(format!(
                    "invalid character '{ch}' in alias"
                )));
            }
        }
        Ok(Self(s))
    }
}

impl TryFrom<String> for Alias {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl From<Alias> for String {
    fn from(a: Alias) -> Self {
        a.0
    }
}

impl fmt::Display for Alias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Alias {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for Alias {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

pub trait AgentStore: Send + Sync {
    fn save(&self, agent: &mut Agent) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &AgentId) -> impl Future<Output = Result<Option<Agent>>> + Send;
    fn find_by_alias(
        &self,
        project: &ProjectId,
        alias: &Alias,
    ) -> impl Future<Output = Result<Option<Agent>>> + Send;
    fn list(&self) -> impl Future<Output = Result<Vec<Agent>>> + Send;
    fn find_timed_out(&self, timeout_secs: u64) -> impl Future<Output = Result<Vec<Agent>>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
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

impl FromStr for AgentStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "online" => Ok(AgentStatus::Online),
            "busy" => Ok(AgentStatus::Busy),
            "idle" => Ok(AgentStatus::Idle),
            "disconnected" => Ok(AgentStatus::Disconnected),
            other => Err(format!("unknown agent status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    id: AgentId,
    project: ProjectId,
    namespace: Namespace,
    parent_id: Option<AgentId>,
    alias: Option<Alias>,
    roles: Vec<String>,
    description: String,
    status: AgentStatus,
    last_heartbeat: DateTime<Utc>,
    connected_at: DateTime<Utc>,
    metadata: HashMap<String, String>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Agent {
    pub fn register(
        project: ProjectId,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
        alias: Option<Alias>,
        metadata: HashMap<String, String>,
    ) -> Self {
        let now = Utc::now();
        let mut agent = Self {
            id: AgentId::new(),
            project,
            namespace,
            parent_id: None,
            alias,
            roles,
            description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata,
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            agent.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_REGISTERED,
            Payload::from_json(&agent_events::AgentRegisteredPayload {
                agent_id: agent.id.to_string(),
                project: agent.project.to_string(),
                namespace: agent.namespace.to_string(),
                roles: agent.roles.clone(),
            })
            .unwrap(),
        )
        .map(|e| agent.collector.collect(e));

        agent
    }

    pub fn from_parent(
        parent: &Agent,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
        alias: Option<Alias>,
    ) -> Self {
        let now = Utc::now();
        let mut agent = Self {
            id: AgentId::new(),
            project: parent.project.clone(),
            namespace,
            parent_id: Some(parent.id),
            alias,
            roles,
            description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata: parent.metadata.clone(),
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            agent.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_SPAWNED,
            Payload::from_json(&agent_events::AgentSpawnedPayload {
                agent_id: agent.id.to_string(),
                parent_id: parent.id.to_string(),
                project: agent.project.to_string(),
                namespace: agent.namespace.to_string(),
                roles: agent.roles.clone(),
            })
            .unwrap(),
        )
        .map(|e| agent.collector.collect(e));

        agent
    }

    pub fn restore(r: RestoreAgent) -> Self {
        Self {
            id: r.id,
            project: r.project,
            namespace: r.namespace,
            parent_id: r.parent_id,
            alias: r.alias,
            roles: r.roles,
            description: r.description,
            status: r.status,
            last_heartbeat: r.last_heartbeat,
            connected_at: r.connected_at,
            metadata: r.metadata,
            collector: EventCollector::new(),
        }
    }

    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Utc::now();
        if self.status == AgentStatus::Disconnected {
            self.status = AgentStatus::Online;

            let _ = Event::create(
                self.project.as_ref(),
                agent_events::NAMESPACE,
                agent_events::TOPIC_STATUS_CHANGED,
                Payload::from_json(&agent_events::AgentStatusChangedPayload {
                    agent_id: self.id.to_string(),
                    status: self.status.to_string(),
                })
                .unwrap(),
            )
            .map(|e| self.collector.collect(e));
        }
    }

    pub fn disconnect(&mut self) {
        self.status = AgentStatus::Disconnected;

        let _ = Event::create(
            self.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_DISCONNECTED,
            Payload::from_json(&agent_events::AgentDisconnectedPayload {
                agent_id: self.id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn update_status(&mut self, status: AgentStatus) {
        self.status = status;

        let _ = Event::create(
            self.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_STATUS_CHANGED,
            Payload::from_json(&agent_events::AgentStatusChangedPayload {
                agent_id: self.id.to_string(),
                status: self.status.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn change_roles(&mut self, roles: Vec<String>) {
        self.roles = roles;

        let _ = Event::create(
            self.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_ROLES_CHANGED,
            Payload::from_json(&agent_events::AgentRolesChangedPayload {
                agent_id: self.id.to_string(),
                roles: self.roles.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn resume(&mut self, namespace: Namespace, roles: Vec<String>, description: String) {
        self.namespace = namespace;
        if !roles.is_empty() {
            self.roles = roles;
        }
        if !description.is_empty() {
            self.description = description;
        }
        self.status = AgentStatus::Online;
        self.last_heartbeat = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_RESUMED,
            Payload::from_json(&agent_events::AgentResumedPayload {
                agent_id: self.id.to_string(),
                namespace: self.namespace.to_string(),
                roles: self.roles.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        self.namespace = namespace;

        let _ = Event::create(
            self.project.as_ref(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_MOVED,
            Payload::from_json(&agent_events::AgentMovedPayload {
                agent_id: self.id.to_string(),
                namespace: self.namespace.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn set_alias(&mut self, alias: Option<Alias>) {
        self.alias = alias;
    }

    pub fn set_metadata(&mut self, metadata: HashMap<String, String>) {
        self.metadata = metadata;
    }

    pub fn is_timed_out(&self, timeout_secs: u64) -> bool {
        self.status != AgentStatus::Disconnected
            && (Utc::now() - self.last_heartbeat) > chrono::Duration::seconds(timeout_secs as i64)
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
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
    pub fn alias(&self) -> Option<&Alias> {
        self.alias.as_ref()
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

pub struct RestoreAgent {
    pub id: AgentId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub parent_id: Option<AgentId>,
    pub alias: Option<Alias>,
    pub roles: Vec<String>,
    pub description: String,
    pub status: AgentStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub connected_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RegisterAgent {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub roles: Vec<String>,
    pub description: String,
    pub alias: Option<Alias>,
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
        Namespace::root()
    }

    fn make_agent() -> Agent {
        Agent::register(
            test_project(),
            test_namespace(),
            vec!["coder".to_string()],
            "test agent".to_string(),
            None,
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
            None,
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

    #[test]
    fn resume_preserves_roles_when_empty() {
        let mut agent = make_agent();
        assert_eq!(agent.roles(), &["coder"]);
        agent.disconnect();
        agent.resume(Namespace::root(), vec![], String::new());
        assert_eq!(agent.status(), AgentStatus::Online);
        assert_eq!(agent.roles(), &["coder"]);
        assert_eq!(agent.description(), "test agent");
    }

    #[test]
    fn resume_overwrites_roles_when_provided() {
        let mut agent = make_agent();
        agent.disconnect();
        agent.resume(
            Namespace::root(),
            vec!["reviewer".to_string()],
            "updated".to_string(),
        );
        assert_eq!(agent.roles(), &["reviewer"]);
        assert_eq!(agent.description(), "updated");
    }
}
