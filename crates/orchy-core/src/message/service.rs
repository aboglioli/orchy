use std::sync::Arc;

use super::{Message, MessageId, MessageStore, MessageTarget};
use crate::agent::{AgentId, AgentStore};
use crate::error::Result;
use crate::namespace::Namespace;

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
        namespace: Namespace,
        from: AgentId,
        to: MessageTarget,
        body: String,
        reply_to: Option<MessageId>,
    ) -> Result<Vec<Message>> {
        match &to {
            MessageTarget::Agent(_) => {
                let msg = Message::new(namespace, from, to, body, reply_to);
                self.message_store.save(&msg).await?;
                Ok(vec![msg])
            }
            MessageTarget::Role(role) => {
                let agents = self.agent_store.list().await?;
                let targets: Vec<AgentId> = agents
                    .into_iter()
                    .filter(|a| a.roles().iter().any(|r| r == role))
                    .map(|a| a.id())
                    .collect();

                let mut sent = Vec::with_capacity(targets.len());
                for target_id in targets {
                    let msg = Message::new(
                        namespace.clone(),
                        from,
                        MessageTarget::Agent(target_id),
                        body.clone(),
                        reply_to,
                    );
                    self.message_store.save(&msg).await?;
                    sent.push(msg);
                }
                Ok(sent)
            }
            MessageTarget::Broadcast => {
                let agents = self.agent_store.list().await?;
                let targets: Vec<AgentId> = agents
                    .into_iter()
                    .filter(|a| a.id() != from)
                    .map(|a| a.id())
                    .collect();

                let mut sent = Vec::with_capacity(targets.len());
                for target_id in targets {
                    let msg = Message::new(
                        namespace.clone(),
                        from,
                        MessageTarget::Agent(target_id),
                        body.clone(),
                        reply_to,
                    );
                    self.message_store.save(&msg).await?;
                    sent.push(msg);
                }
                Ok(sent)
            }
        }
    }

    pub async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        let mut messages = self.message_store.find_pending(agent, namespace).await?;
        for msg in &mut messages {
            msg.deliver();
            self.message_store.save(msg).await?;
        }
        Ok(messages)
    }

    pub async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        for id in ids {
            if let Some(mut msg) = self.message_store.find_by_id(id).await? {
                msg.mark_read();
                self.message_store.save(&msg).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AgentStore};
    use crate::namespace::{Namespace, ProjectId};
    use crate::store::mock::MockStore;
    use std::collections::HashMap;

    fn make_agent(roles: Vec<&str>) -> Agent {
        Agent::register(
            ProjectId::try_from("orchy").unwrap(),
            Namespace::try_from("orchy").unwrap(),
            roles.into_iter().map(String::from).collect(),
            "test".to_string(),
            HashMap::new(),
        )
    }

    async fn save_agent(store: &MockStore, agent: &Agent) {
        AgentStore::save(store, agent).await.unwrap();
    }

    fn ns() -> Namespace {
        Namespace::try_from("orchy".to_string()).unwrap()
    }

    #[tokio::test]
    async fn send_to_agent_returns_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                ns(),
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
        let a1 = make_agent(vec!["tester"]);
        let a2 = make_agent(vec!["developer"]);
        save_agent(&store, &a1).await;
        save_agent(&store, &a2).await;
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                ns(),
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
        let a = make_agent(vec!["developer"]);
        save_agent(&store, &a).await;
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                ns(),
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
        let sender = make_agent(vec!["tester"]);
        let other = make_agent(vec!["tester"]);
        save_agent(&store, &sender).await;
        save_agent(&store, &other).await;
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(
                ns(),
                sender.id(),
                MessageTarget::Broadcast,
                "hi".into(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }
}
