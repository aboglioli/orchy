use std::sync::Arc;

use super::{CreateMessage, Message, MessageStore, MessageTarget};
use crate::agent::{AgentId, AgentStore};
use crate::error::Result;
use crate::message::MessageId;
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

    pub async fn send(&self, cmd: CreateMessage) -> Result<Vec<Message>> {
        match &cmd.to {
            MessageTarget::Agent(_) => {
                let msg = self.message_store.send(cmd).await?;
                Ok(vec![msg])
            }
            MessageTarget::Role(role) => {
                let agents = self.agent_store.list().await?;
                let targets: Vec<AgentId> = agents
                    .into_iter()
                    .filter(|a| a.roles.iter().any(|r| r == role))
                    .map(|a| a.id)
                    .collect();

                let mut sent = Vec::with_capacity(targets.len());
                for target_id in targets {
                    let individual = CreateMessage {
                        namespace: cmd.namespace.clone(),
                        from: cmd.from,
                        to: MessageTarget::Agent(target_id),
                        body: cmd.body.clone(),
                        reply_to: cmd.reply_to,
                    };
                    sent.push(self.message_store.send(individual).await?);
                }
                Ok(sent)
            }
            MessageTarget::Broadcast => {
                let agents = self.agent_store.list().await?;
                let targets: Vec<AgentId> = agents
                    .into_iter()
                    .filter(|a| a.id != cmd.from)
                    .map(|a| a.id)
                    .collect();

                let mut sent = Vec::with_capacity(targets.len());
                for target_id in targets {
                    let individual = CreateMessage {
                        namespace: cmd.namespace.clone(),
                        from: cmd.from,
                        to: MessageTarget::Agent(target_id),
                        body: cmd.body.clone(),
                        reply_to: cmd.reply_to,
                    };
                    sent.push(self.message_store.send(individual).await?);
                }
                Ok(sent)
            }
        }
    }

    pub async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        self.message_store.check(agent, namespace).await
    }

    pub async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        self.message_store.mark_read(ids).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RegisterAgent;
    use crate::store::mock::MockStore;
    use std::collections::HashMap;

    fn make_registration(roles: Vec<&str>) -> RegisterAgent {
        RegisterAgent {
            namespace: Namespace::try_from("orchy".to_string()).unwrap(),
            roles: roles.into_iter().map(String::from).collect(),
            description: "test".to_string(),
            metadata: HashMap::new(),
        }
    }

    fn make_msg(from: AgentId, to: MessageTarget) -> CreateMessage {
        CreateMessage {
            namespace: Namespace::try_from("orchy".to_string()).unwrap(),
            from,
            to,
            body: "test".to_string(),
            reply_to: None,
        }
    }

    #[tokio::test]
    async fn send_to_agent_returns_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(make_msg(
                AgentId::new(),
                MessageTarget::Agent(AgentId::new()),
            ))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn send_to_role_routes_to_matching_agents() {
        let store = Arc::new(MockStore::default());
        store
            .register(make_registration(vec!["tester"]))
            .await
            .unwrap();
        store
            .register(make_registration(vec!["developer"]))
            .await
            .unwrap();
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(make_msg(
                AgentId::new(),
                MessageTarget::Role("tester".to_string()),
            ))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn send_to_role_with_no_matching_agents_returns_empty() {
        let store = Arc::new(MockStore::default());
        store
            .register(make_registration(vec!["developer"]))
            .await
            .unwrap();
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(make_msg(
                AgentId::new(),
                MessageTarget::Role("tester".to_string()),
            ))
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn send_to_role_delivers_to_all_matching_agents() {
        let store = Arc::new(MockStore::default());
        store
            .register(make_registration(vec!["tester"]))
            .await
            .unwrap();
        store
            .register(make_registration(vec!["tester"]))
            .await
            .unwrap();
        store
            .register(make_registration(vec!["developer"]))
            .await
            .unwrap();
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(make_msg(
                AgentId::new(),
                MessageTarget::Role("tester".to_string()),
            ))
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn broadcast_excludes_sender() {
        let store = Arc::new(MockStore::default());
        let sender = store
            .register(make_registration(vec!["tester"]))
            .await
            .unwrap();
        store
            .register(make_registration(vec!["tester"]))
            .await
            .unwrap();
        let service = MessageService::new(Arc::clone(&store), store);
        let result = service
            .send(make_msg(sender.id, MessageTarget::Broadcast))
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn check_returns_agent_messages() {
        let store = Arc::new(MockStore::default());
        let agent = store
            .register(make_registration(vec!["tester"]))
            .await
            .unwrap();
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let service = MessageService::new(Arc::clone(&store), store);
        service
            .send(make_msg(AgentId::new(), MessageTarget::Agent(agent.id)))
            .await
            .unwrap();
        let result = service.check(&agent.id, &ns).await.unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn mark_read_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(Arc::clone(&store), store);
        assert!(service.mark_read(&[MessageId::new()]).await.is_ok());
    }
}
