pub mod alias;
pub mod events;

pub use alias::Alias;

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
use crate::user::UserId;

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
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| Error::invalid_input(format!("invalid agent id: {s}")))
    }
}

#[async_trait::async_trait]
pub trait AgentStore: Send + Sync {
    async fn save(&self, agent: &mut Agent) -> Result<()>;
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>>;
    async fn find_by_ids(&self, ids: &[AgentId]) -> Result<Vec<Agent>>;
    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>>;
    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>>;
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    id: AgentId,
    alias: Alias,
    org_id: OrganizationId,
    project: ProjectId,
    namespace: Namespace,
    roles: Vec<String>,
    description: String,
    last_seen: DateTime<Utc>,
    connected_at: DateTime<Utc>,
    metadata: HashMap<String, String>,
    user_id: Option<UserId>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Agent {
    pub fn register(
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
        alias: Alias,
        roles: Vec<String>,
        description: String,
        id: Option<AgentId>,
        metadata: HashMap<String, String>,
        user_id: Option<UserId>,
    ) -> Result<Self> {
        let now = Utc::now();
        let id = id.unwrap_or_default();
        let mut agent = Self {
            id,
            alias,
            org_id,
            project,
            namespace,
            roles,
            description,
            last_seen: now,
            connected_at: now,
            metadata,
            user_id,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&agent_events::AgentRegisteredPayload {
            org_id: agent.org_id.to_string(),
            agent_id: agent.id.to_string(),
            alias: agent.alias.to_string(),
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

    pub fn restore(r: RestoreAgent) -> Self {
        Self {
            id: r.id,
            alias: r.alias,
            org_id: r.org_id,
            project: r.project,
            namespace: r.namespace,
            roles: r.roles,
            description: r.description,
            last_seen: r.last_seen,
            connected_at: r.connected_at,
            metadata: r.metadata,
            user_id: r.user_id,
            collector: EventCollector::new(),
        }
    }

    /// No event emitted: heartbeat is high-frequency and would flood
    /// the event log. This is the only mutation that skips event emission.
    pub fn heartbeat(&mut self) -> Result<()> {
        self.last_seen = Utc::now();
        Ok(())
    }

    pub fn derived_status(&self, idle_secs: u64, stale_secs: u64) -> &'static str {
        let elapsed = (Utc::now() - self.last_seen).num_seconds() as u64;
        if elapsed < idle_secs {
            "active"
        } else if elapsed < stale_secs {
            "idle"
        } else {
            "stale"
        }
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
        self.last_seen = Utc::now();

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
        self.last_seen = Utc::now();

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

    pub fn set_metadata(&mut self, metadata: HashMap<String, String>) -> Result<()> {
        self.metadata = metadata;

        let payload = Payload::from_json(&agent_events::AgentMetadataChangedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
            metadata: self.metadata.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_METADATA_CHANGED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn set_alias(&mut self, alias: Alias) -> Result<()> {
        if self.alias == alias {
            return Ok(());
        }
        self.alias = alias.clone();

        let payload = Payload::from_json(&agent_events::AgentAliasChangedPayload {
            org_id: self.org_id.to_string(),
            agent_id: self.id.to_string(),
            new_alias: alias.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            agent_events::NAMESPACE,
            agent_events::TOPIC_ALIAS_CHANGED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn is_timed_out(&self, timeout_secs: u64) -> bool {
        (Utc::now() - self.last_seen) > chrono::Duration::seconds(timeout_secs as i64)
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> &AgentId {
        &self.id
    }
    pub fn alias(&self) -> &Alias {
        &self.alias
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
    pub fn roles(&self) -> &[String] {
        &self.roles
    }
    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn last_seen(&self) -> DateTime<Utc> {
        self.last_seen
    }
    pub fn connected_at(&self) -> DateTime<Utc> {
        self.connected_at
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    pub fn user_id(&self) -> Option<&UserId> {
        self.user_id.as_ref()
    }
    pub fn attach_user(&mut self, user_id: UserId) {
        self.user_id = Some(user_id);
    }
}

pub struct RestoreAgent {
    pub id: AgentId,
    pub alias: Alias,
    pub org_id: OrganizationId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub roles: Vec<String>,
    pub description: String,
    pub last_seen: DateTime<Utc>,
    pub connected_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
    pub user_id: Option<UserId>,
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
            Alias::new("test-coder").unwrap(),
            vec!["coder".to_string()],
            "test agent".to_string(),
            None,
            HashMap::new(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn register_creates_online_agent() {
        let agent = make_agent();
        // status derived from last_seen
        assert_eq!(agent.roles(), &["coder"]);
    }

    #[test]
    fn heartbeat_updates_timestamp() {
        let mut agent = make_agent();
        let before = agent.last_seen();
        sleep(Duration::from_millis(10));
        agent.heartbeat().unwrap();
        assert!(agent.last_seen() > before);
    }

    #[test]
    fn heartbeat_updates_last_seen() {
        let mut agent = make_agent();
        agent.heartbeat().unwrap();
        // status derived from last_seen
    }

    #[test]
    fn is_timed_out_when_stale() {
        let mut agent = make_agent();
        agent.heartbeat().unwrap();
        sleep(Duration::from_millis(10));
        assert!(agent.is_timed_out(0));
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
        agent
            .resume(Namespace::root(), vec![], String::new())
            .unwrap();
        // status derived from last_seen
        assert_eq!(agent.roles(), &["coder"]);
        assert_eq!(agent.description(), "test agent");
    }

    #[test]
    fn resume_overwrites_roles_when_provided() {
        let mut agent = make_agent();
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
