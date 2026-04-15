use std::sync::Arc;

use super::{Message, MessageId, MessageStore, MessageTarget};
use crate::agent::{AgentId, AgentStore};
use crate::error::Result;
use crate::namespace::{Namespace, ProjectId};

pub struct MessageService<MS: MessageStore, AS: AgentStore> {
    message_store: Arc<MS>,
    agent_store: Arc<AS>,
}

impl<MS: MessageStore, AS: AgentStore> MessageService<MS, AS> {
    pub fn new(message_store: Arc<MS>, agent_store: Arc<AS>) -> Self {
        Self {
            message_store,
            agent_store,
        }
    }

    pub async fn send(
        &self,
        project: ProjectId,
        namespace: Namespace,
        from: AgentId,
        to: MessageTarget,
        body: String,
        reply_to: Option<MessageId>,
    ) -> Result<Vec<Message>> {
        let targets = match &to {
            MessageTarget::Agent(_) => {
                let mut msg = Message::new(project, namespace, from, to, body, reply_to);
                self.message_store.save(&mut msg).await?;
                return Ok(vec![msg]);
            }
            MessageTarget::Role(role) => {
                let agents = self.agent_store.list().await?;
                agents
                    .into_iter()
                    .filter(|a| a.project() == &project)
                    .filter(|a| a.roles().iter().any(|r| r == role))
                    .map(|a| a.id())
                    .collect::<Vec<_>>()
            }
            MessageTarget::Broadcast => {
                let agents = self.agent_store.list().await?;
                agents
                    .into_iter()
                    .filter(|a| a.project() == &project)
                    .filter(|a| a.id() != from)
                    .map(|a| a.id())
                    .collect::<Vec<_>>()
            }
        };

        let mut sent = Vec::with_capacity(targets.len());
        for target_id in targets {
            let mut msg = Message::new(
                project.clone(),
                namespace.clone(),
                from,
                MessageTarget::Agent(target_id),
                body.clone(),
                reply_to,
            );
            self.message_store.save(&mut msg).await?;
            sent.push(msg);
        }
        Ok(sent)
    }

    pub async fn check(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let mut messages = self
            .message_store
            .find_pending(agent, project, namespace)
            .await?;
        for msg in &mut messages {
            msg.deliver();
            self.message_store.save(msg).await?;
        }
        Ok(messages)
    }

    pub async fn sent(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        self.message_store
            .find_sent(agent, project, namespace)
            .await
    }

    pub async fn thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.message_store.find_thread(message_id, limit).await
    }

    pub async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        for id in ids {
            if let Some(mut msg) = self.message_store.find_by_id(id).await? {
                msg.mark_read();
                self.message_store.save(&mut msg).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AgentStore};
    use crate::infrastructure::MockStore;
    use crate::namespace::{Namespace, ProjectId};
    use std::collections::HashMap;

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    fn make_agent(roles: Vec<&str>) -> Agent {
        make_agent_for_project(test_project(), roles)
    }

    fn other_project() -> ProjectId {
        ProjectId::try_from("other").unwrap()
    }

    fn make_agent_for_project(project: ProjectId, roles: Vec<&str>) -> Agent {
        Agent::register(
            project,
            Namespace::root(),
            roles.into_iter().map(String::from).collect(),
            "test".to_string(),
            HashMap::new(),
        )
    }

    async fn save_agent(store: &MockStore, agent: &mut Agent) {
        AgentStore::save(store, agent).await.unwrap();
    }

    #[tokio::test]
    async fn send_to_agent_returns_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                test_project(),
                Namespace::root(),
                AgentId::new(),
                MessageTarget::Agent(AgentId::new()),
                "hi".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn send_to_role_routes_to_matching_agents() {
        let store = Arc::new(MockStore::default());
        let mut a1 = make_agent(vec!["tester"]);
        let mut a2 = make_agent(vec!["developer"]);
        save_agent(&store, &mut a1).await;
        save_agent(&store, &mut a2).await;
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                test_project(),
                Namespace::root(),
                AgentId::new(),
                MessageTarget::Role("tester".to_string()),
                "hi".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn send_to_role_with_no_matching_agents_returns_empty() {
        let store = Arc::new(MockStore::default());
        let mut a = make_agent(vec!["developer"]);
        save_agent(&store, &mut a).await;
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                test_project(),
                Namespace::root(),
                AgentId::new(),
                MessageTarget::Role("tester".to_string()),
                "hi".into(),
                None,
            )
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn broadcast_excludes_sender() {
        let store = Arc::new(MockStore::default());
        let mut sender = make_agent(vec!["tester"]);
        let mut other = make_agent(vec!["tester"]);
        save_agent(&store, &mut sender).await;
        save_agent(&store, &mut other).await;
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                test_project(),
                Namespace::root(),
                sender.id(),
                MessageTarget::Broadcast,
                "hi".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn send_to_role_skips_agents_from_other_projects() {
        let store = Arc::new(MockStore::default());
        let mut local = make_agent(vec!["reviewer"]);
        let mut foreign = make_agent_for_project(other_project(), vec!["reviewer"]);
        save_agent(&store, &mut local).await;
        save_agent(&store, &mut foreign).await;
        let service = MessageService::new(Arc::clone(&store), Arc::clone(&store));

        let result = service
            .send(
                test_project(),
                Namespace::root(),
                AgentId::new(),
                MessageTarget::Role("reviewer".to_string()),
                "hi".into(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to(), &MessageTarget::Agent(local.id()));
    }

    #[tokio::test]
    async fn broadcast_skips_agents_from_other_projects() {
        let store = Arc::new(MockStore::default());
        let mut sender = make_agent(vec!["tester"]);
        let mut local = make_agent(vec!["tester"]);
        let mut foreign = make_agent_for_project(other_project(), vec!["tester"]);
        save_agent(&store, &mut sender).await;
        save_agent(&store, &mut local).await;
        save_agent(&store, &mut foreign).await;
        let service = MessageService::new(Arc::clone(&store), Arc::clone(&store));

        let result = service
            .send(
                test_project(),
                Namespace::root(),
                sender.id(),
                MessageTarget::Broadcast,
                "hi".into(),
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to(), &MessageTarget::Agent(local.id()));
    }
}
