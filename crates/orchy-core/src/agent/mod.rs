pub mod events;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};

use self::events as agent_events;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
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
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self).map_err(|e| e.to_string())
    }
}

#[async_trait::async_trait]
pub trait AgentStore: Send + Sync {
    async fn save(&self, agent: &mut Agent) -> Result<()>;
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>>;
    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>>;
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>>;
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
    org_id: OrganizationId,
    project: ProjectId,
    namespace: Namespace,
    parent_id: Option<AgentId>,
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
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
        id: Option<AgentId>,
        metadata: HashMap<String, String>,
    ) -> Result<Self> {
        let now = Utc::now();
        let id = id.unwrap_or_default();
        let mut agent = Self {
            id,
            org_id,
            project,
            namespace,
            parent_id: None,
            roles,
            description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&agent_events::AgentRegisteredPayload {
            org_id: agent.org_id.to_string(),
            agent_id: agent.id.to_string(),
            project: agent.project.to_string(),
            namespace: agent.namespace.to_string(),
            roles: agent.roles.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            agent.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_REGISTERED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        agent.collector.collect(event);

        Ok(agent)
    }

    pub fn from_parent(
        parent: &Agent,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
        id: Option<AgentId>,
    ) -> Result<Self> {
        let now = Utc::now();
        let id = id.unwrap_or_default();
        let mut agent = Self {
            id,
            org_id: parent.org_id.clone(),
            project: parent.project.clone(),
            namespace,
            parent_id: Some(parent.id.clone()),
            roles,
            description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata: parent.metadata.clone(),
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&agent_events::AgentSpawnedPayload {
            org_id: agent.org_id.to_string(),
            agent_id: agent.id.to_string(),
            parent_id: parent.id.to_string(),
            project: agent.project.to_string(),
            namespace: agent.namespace.to_string(),
            roles: agent.roles.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            agent.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_SPAWNED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        agent.collector.collect(event);

        Ok(agent)
    }

    pub fn restore(r: RestoreAgent) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            project: r.project,
            namespace: r.namespace,
            parent_id: r.parent_id,
            roles: r.roles,
            description: r.description,
            status: r.status,
            last_heartbeat: r.last_heartbeat,
            connected_at: r.connected_at,
            metadata: r.metadata,
            collector: EventCollector::new(),
        }
    }

    pub fn heartbeat(&mut self) -> Result<()> {
        self.last_heartbeat = Utc::now();
        if self.status == AgentStatus::Disconnected {
            self.status = AgentStatus::Online;

            let payload = Payload::from_json(&agent_events::AgentStatusChangedPayload {
                org_id: self.org_id.to_string(),
                agent_id: self.id.to_string(),
                status: self.status.to_string(),
            })
            .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
            let event = Event::create(
                self.org_id.as_str(),
                agent_events::NAMESPACE,
                agent_events::TOPIC_STATUS_CHANGED,
                payload,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?;
            self.collector.collect(event);
        }
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        self.status = AgentStatus::Disconnected;

        let payload = Payload::from_json(&agent_events::AgentDisconnectedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_DISCONNECTED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn update_status(&mut self, status: AgentStatus) -> Result<()> {
        self.status = status;

        let payload = Payload::from_json(&agent_events::AgentStatusChangedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
            status: self.status.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_STATUS_CHANGED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn change_roles(&mut self, roles: Vec<String>) -> Result<()> {
        self.roles = roles;

        let payload = Payload::from_json(&agent_events::AgentRolesChangedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
            roles: self.roles.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_ROLES_CHANGED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn resume(
        &mut self,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
    ) -> Result<()> {
        self.namespace = namespace;
        if !roles.is_empty() {
            self.roles = roles;
        }
        if !description.is_empty() {
            self.description = description;
        }
        self.status = AgentStatus::Online;
        self.last_heartbeat = Utc::now();

        let payload = Payload::from_json(&agent_events::AgentResumedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
            namespace: self.namespace.to_string(),
            roles: self.roles.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_RESUMED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn switch_context(
        &mut self,
        project: Option<ProjectId>,
        namespace: Namespace,
    ) -> Result<()> {
        let old_project = self.project.to_string();
        let old_namespace = self.namespace.to_string();

        if let Some(p) = project {
            self.project = p;
        }
        self.namespace = namespace;

        if old_project == self.project.to_string() && old_namespace == self.namespace.to_string() {
            return Ok(());
        }

        let payload = Payload::from_json(&agent_events::AgentContextSwitchedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
            old_project,
            new_project: self.project.to_string(),
            old_namespace,
            new_namespace: self.namespace.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_CONTEXT_SWITCHED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
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

    pub fn id(&self) -> &AgentId {
        &self.id
    }
    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn parent_id(&self) -> Option<&AgentId> {
        self.parent_id.as_ref()
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
    pub org_id: OrganizationId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub parent_id: Option<AgentId>,
    pub roles: Vec<String>,
    pub description: String,
    pub status: AgentStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub connected_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RegisterAgent {
    pub org_id: OrganizationId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub roles: Vec<String>,
    pub description: String,
    pub id: Option<AgentId>,
    pub parent_id: Option<AgentId>,
    pub metadata: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn test_org() -> OrganizationId {
        OrganizationId::new("test").unwrap()
    }

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    fn test_namespace() -> Namespace {
        Namespace::root()
    }

    fn make_agent() -> Agent {
        Agent::register(
            test_org(),
            test_project(),
            test_namespace(),
            vec!["coder".to_string()],
            "test agent".to_string(),
            None,
            HashMap::new(),
        )
        .unwrap()
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
        )
        .unwrap();
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
        agent.heartbeat().unwrap();
        assert!(agent.last_heartbeat() > before);
    }

    #[test]
    fn heartbeat_reconnects_disconnected() {
        let mut agent = make_agent();
        agent.disconnect().unwrap();
        agent.heartbeat().unwrap();
        assert_eq!(agent.status(), AgentStatus::Online);
    }

    #[test]
    fn disconnect_sets_status() {
        let mut agent = make_agent();
        agent.disconnect().unwrap();
        assert_eq!(agent.status(), AgentStatus::Disconnected);
    }

    #[test]
    fn is_timed_out_when_stale() {
        let mut agent = make_agent();
        agent.heartbeat().unwrap();
        sleep(Duration::from_millis(10));
        assert!(agent.is_timed_out(0));
    }

    #[test]
    fn is_timed_out_false_when_disconnected() {
        let mut agent = make_agent();
        agent.disconnect().unwrap();
        sleep(Duration::from_millis(10));
        assert!(!agent.is_timed_out(0));
    }

    #[test]
    fn switch_context_changes_project_and_namespace() {
        let mut agent = make_agent();
        let new_project = ProjectId::try_from("other").unwrap();
        let new_ns = Namespace::try_from("/frontend".to_string()).unwrap();
        agent.switch_context(Some(new_project), new_ns).unwrap();
        assert_eq!(agent.project().as_ref(), "other");
        assert_eq!(agent.namespace().to_string(), "/frontend");
    }

    #[test]
    fn switch_context_namespace_only() {
        let mut agent = make_agent();
        let new_ns = Namespace::try_from("/backend".to_string()).unwrap();
        agent.switch_context(None, new_ns).unwrap();
        assert_eq!(agent.project().as_ref(), "test");
        assert_eq!(agent.namespace().to_string(), "/backend");
    }

    #[test]
    fn switch_context_noop_when_unchanged() {
        let mut agent = make_agent();
        let _ = agent.drain_events();
        agent.switch_context(None, Namespace::root()).unwrap();
        let events = agent.drain_events();
        assert!(events.is_empty());
    }

    #[test]
    fn resume_preserves_roles_when_empty() {
        let mut agent = make_agent();
        assert_eq!(agent.roles(), &["coder"]);
        agent.disconnect().unwrap();
        agent
            .resume(Namespace::root(), vec![], String::new())
            .unwrap();
        assert_eq!(agent.status(), AgentStatus::Online);
        assert_eq!(agent.roles(), &["coder"]);
        assert_eq!(agent.description(), "test agent");
    }

    #[test]
    fn resume_overwrites_roles_when_provided() {
        let mut agent = make_agent();
        agent.disconnect().unwrap();
        agent
            .resume(
                Namespace::root(),
                vec!["reviewer".to_string()],
                "updated".to_string(),
            )
            .unwrap();
        assert_eq!(agent.roles(), &["reviewer"]);
        assert_eq!(agent.description(), "updated");
    }
}
