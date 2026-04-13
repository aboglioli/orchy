#[cfg(test)]
pub mod mock {
    use std::collections::HashMap;
    use std::sync::RwLock;

    use crate::agent::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
    use crate::error::{Error, Result};
    use crate::memory::{
        ContextSnapshot, ContextStore, CreateSnapshot, MemoryEntry, MemoryFilter, MemoryStore,
        WriteMemory,
    };
    use crate::message::{
        CreateMessage, Message, MessageId, MessageStatus, MessageStore, MessageTarget,
    };
    use crate::namespace::{Namespace, ProjectId};
    use crate::project::{Project, ProjectStore};
    use crate::skill::{Skill, SkillFilter, SkillStore, WriteSkill};
    use crate::task::{Task, TaskFilter, TaskId, TaskStore};

    #[derive(Debug, Default)]
    pub struct MockStore {
        agents: RwLock<HashMap<AgentId, Agent>>,
        messages: RwLock<Vec<Message>>,
    }

    impl TaskStore for MockStore {
        async fn save(&self, _: &Task) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &TaskId) -> Result<Option<Task>> {
            unimplemented!()
        }
        async fn list(&self, _: TaskFilter) -> Result<Vec<Task>> {
            unimplemented!()
        }
    }

    impl AgentStore for MockStore {
        async fn register(&self, reg: RegisterAgent) -> Result<Agent> {
            let agent = Agent {
                id: AgentId::new(),
                namespace: reg.namespace,
                roles: reg.roles,
                description: reg.description,
                status: AgentStatus::Online,
                last_heartbeat: chrono::Utc::now(),
                connected_at: chrono::Utc::now(),
                metadata: reg.metadata,
            };
            self.agents.write().unwrap().insert(agent.id, agent.clone());
            Ok(agent)
        }
        async fn get(&self, id: &AgentId) -> Result<Option<Agent>> {
            Ok(self.agents.read().unwrap().get(id).cloned())
        }
        async fn list(&self) -> Result<Vec<Agent>> {
            Ok(self.agents.read().unwrap().values().cloned().collect())
        }
        async fn heartbeat(&self, _: &AgentId) -> Result<()> {
            Ok(())
        }
        async fn update_status(&self, _: &AgentId, _: AgentStatus) -> Result<()> {
            Ok(())
        }
        async fn update_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
            let mut agents = self.agents.write().unwrap();
            let agent = agents
                .get_mut(id)
                .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
            agent.roles = roles;
            Ok(agent.clone())
        }
        async fn reconnect(
            &self,
            id: &AgentId,
            roles: Vec<String>,
            description: String,
        ) -> Result<Agent> {
            let mut agents = self.agents.write().unwrap();
            let agent = agents
                .get_mut(id)
                .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
            agent.status = AgentStatus::Online;
            agent.roles = roles;
            agent.description = description;
            agent.last_heartbeat = chrono::Utc::now();
            Ok(agent.clone())
        }
        async fn disconnect(&self, _: &AgentId) -> Result<()> {
            Ok(())
        }
        async fn find_timed_out(&self, _: u64) -> Result<Vec<Agent>> {
            Ok(vec![])
        }
    }

    impl MessageStore for MockStore {
        async fn send(&self, cmd: CreateMessage) -> Result<Message> {
            let msg = Message {
                id: MessageId::new(),
                namespace: cmd.namespace,
                from: cmd.from,
                to: cmd.to,
                body: cmd.body,
                reply_to: cmd.reply_to,
                status: MessageStatus::Pending,
                created_at: chrono::Utc::now(),
            };
            self.messages.write().unwrap().push(msg.clone());
            Ok(msg)
        }
        async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
            Ok(self
                .messages
                .read()
                .unwrap()
                .iter()
                .filter(|m| m.to == MessageTarget::Agent(*agent) && m.namespace == *namespace)
                .cloned()
                .collect())
        }
        async fn mark_read(&self, _: &[MessageId]) -> Result<()> {
            Ok(())
        }
    }

    impl ProjectStore for MockStore {
        async fn save(&self, _: &Project) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &ProjectId) -> Result<Option<Project>> {
            Ok(None)
        }
    }

    impl MemoryStore for MockStore {
        async fn write(&self, _: WriteMemory) -> Result<MemoryEntry> {
            unimplemented!()
        }
        async fn read(&self, _: &Namespace, _: &str) -> Result<Option<MemoryEntry>> {
            unimplemented!()
        }
        async fn list(&self, _: MemoryFilter) -> Result<Vec<MemoryEntry>> {
            unimplemented!()
        }
        async fn search(
            &self,
            _: &str,
            _: Option<&[f32]>,
            _: Option<&Namespace>,
            _: usize,
        ) -> Result<Vec<MemoryEntry>> {
            unimplemented!()
        }
        async fn delete(&self, _: &Namespace, _: &str) -> Result<()> {
            unimplemented!()
        }
    }

    impl ContextStore for MockStore {
        async fn save(&self, _: CreateSnapshot) -> Result<ContextSnapshot> {
            unimplemented!()
        }
        async fn load(&self, _: &AgentId) -> Result<Option<ContextSnapshot>> {
            unimplemented!()
        }
        async fn list(&self, _: Option<&AgentId>, _: &Namespace) -> Result<Vec<ContextSnapshot>> {
            unimplemented!()
        }
        async fn search(
            &self,
            _: &str,
            _: Option<&[f32]>,
            _: &Namespace,
            _: Option<&AgentId>,
            _: usize,
        ) -> Result<Vec<ContextSnapshot>> {
            unimplemented!()
        }
    }

    impl SkillStore for MockStore {
        async fn write(&self, _: WriteSkill) -> Result<Skill> {
            unimplemented!()
        }
        async fn read(&self, _: &Namespace, _: &str) -> Result<Option<Skill>> {
            unimplemented!()
        }
        async fn list(&self, _: SkillFilter) -> Result<Vec<Skill>> {
            unimplemented!()
        }
        async fn delete(&self, _: &Namespace, _: &str) -> Result<()> {
            unimplemented!()
        }
    }
}
