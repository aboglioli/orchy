use std::sync::Arc;

use super::{CreateMessage, Message, MessageTarget};
use crate::agent::AgentId;
use crate::error::Result;
use crate::message::MessageId;
use crate::namespace::Namespace;
use crate::store::Store;

pub struct MessageService<S: Store> {
    store: Arc<S>,
}

impl<S: Store> MessageService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn send(&self, cmd: CreateMessage) -> Result<Vec<Message>> {
        match &cmd.to {
            MessageTarget::Agent(_) => {
                let msg = self.store.send_message(cmd).await?;
                Ok(vec![msg])
            }
            MessageTarget::Role(role) => {
                let agents = self.store.list_agents().await?;
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
                    };
                    sent.push(self.store.send_message(individual).await?);
                }
                Ok(sent)
            }
            MessageTarget::Broadcast => {
                let agents = self.store.list_agents().await?;
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
                    };
                    sent.push(self.store.send_message(individual).await?);
                }
                Ok(sent)
            }
        }
    }

    pub async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        self.store.check_messages(agent, namespace).await
    }

    pub async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        self.store.mark_messages_read(ids).await
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
        }
    }

    #[tokio::test]
    async fn send_to_agent_returns_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(store);
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
        let service = MessageService::new(store);
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
        let service = MessageService::new(store);
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
        let service = MessageService::new(store);
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
        let service = MessageService::new(store);
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
        let service = MessageService::new(store);
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
        let service = MessageService::new(store);
        assert!(service.mark_read(&[MessageId::new()]).await.is_ok());
    }
}
