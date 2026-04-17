use std::sync::Arc;

use super::{Message, MessageId, MessageStore, MessageTarget};
use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};

pub struct MessageService<MS: MessageStore> {
    message_store: Arc<MS>,
}

pub struct SendMessage {
    pub org_id: OrganizationId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
    pub reply_to: Option<MessageId>,
}

impl<MS: MessageStore> MessageService<MS> {
    pub fn new(message_store: Arc<MS>) -> Self {
        Self { message_store }
    }

    pub async fn send(&self, cmd: SendMessage) -> Result<Vec<Message>> {
        let SendMessage {
            org_id,
            project,
            namespace,
            from,
            to,
            body,
            reply_to,
        } = cmd;

        let mut msg = Message::new(org_id, project, namespace, from, to, body, reply_to)?;
        self.message_store.save(&mut msg).await?;
        Ok(vec![msg])
    }

    pub async fn pending(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        self.message_store
            .find_pending(agent, agent_roles, org, project, namespace, page)
            .await
    }

    pub async fn check(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let mut result = self
            .message_store
            .find_pending(agent, agent_roles, org, project, namespace, page)
            .await?;
        for msg in &mut result.items {
            if msg.is_directed_to(agent) {
                msg.deliver()?;
                self.message_store.save(msg).await?;
            }
        }
        Ok(result)
    }

    pub async fn sent(
        &self,
        agent: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        self.message_store
            .find_sent(agent, org, project, namespace, page)
            .await
    }

    pub async fn thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        self.message_store.find_thread(message_id, limit).await
    }

    pub async fn mark_read(&self, agent: &AgentId, ids: &[MessageId]) -> Result<()> {
        for id in ids {
            if let Some(mut msg) = self.message_store.find_by_id(id).await? {
                if msg.is_directed_to(agent) {
                    msg.mark_read()?;
                    self.message_store.save(&mut msg).await?;
                    continue;
                }

                if msg.is_broadcast() || msg.is_role_targeted() {
                    self.message_store.mark_read_for_agent(id, agent).await?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::MockStore;
    use crate::namespace::Namespace;

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    fn test_org() -> OrganizationId {
        OrganizationId::new("test").unwrap()
    }

    #[tokio::test]
    async fn send_to_agent_returns_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(store);
        let result = service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: AgentId::new(),
                to: MessageTarget::Agent(AgentId::new()),
                body: "hi".into(),
                reply_to: None,
            })
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn send_to_role_stores_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(Arc::clone(&store));
        let result = service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: AgentId::new(),
                to: MessageTarget::Role("tester".to_string()),
                body: "hi".into(),
                reply_to: None,
            })
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to(), &MessageTarget::Role("tester".to_string()));
    }

    #[tokio::test]
    async fn role_message_appears_in_pending_for_matching_agent() {
        let store = Arc::new(MockStore::default());
        let sender = AgentId::new();
        let receiver = AgentId::new();

        let service = MessageService::new(Arc::clone(&store));
        service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: sender,
                to: MessageTarget::Role("reviewer".to_string()),
                body: "review please".into(),
                reply_to: None,
            })
            .await
            .unwrap();

        let pending = service
            .pending(
                &receiver,
                &["reviewer".to_string()],
                &test_org(),
                &test_project(),
                &Namespace::root(),
                PageParams::unbounded(),
            )
            .await
            .unwrap();
        assert_eq!(pending.items.len(), 1);
    }

    #[tokio::test]
    async fn role_message_hidden_from_non_matching_agent() {
        let store = Arc::new(MockStore::default());
        let sender = AgentId::new();
        let receiver = AgentId::new();

        let service = MessageService::new(Arc::clone(&store));
        service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: sender,
                to: MessageTarget::Role("reviewer".to_string()),
                body: "review please".into(),
                reply_to: None,
            })
            .await
            .unwrap();

        let pending = service
            .pending(
                &receiver,
                &["developer".to_string()],
                &test_org(),
                &test_project(),
                &Namespace::root(),
                PageParams::unbounded(),
            )
            .await
            .unwrap();
        assert!(pending.items.is_empty());
    }

    #[tokio::test]
    async fn role_message_excluded_from_sender() {
        let store = Arc::new(MockStore::default());
        let sender = AgentId::new();

        let service = MessageService::new(Arc::clone(&store));
        service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: sender.clone(),
                to: MessageTarget::Role("reviewer".to_string()),
                body: "review please".into(),
                reply_to: None,
            })
            .await
            .unwrap();

        let pending = service
            .pending(
                &sender,
                &["reviewer".to_string()],
                &test_org(),
                &test_project(),
                &Namespace::root(),
                PageParams::unbounded(),
            )
            .await
            .unwrap();
        assert!(pending.items.is_empty());
    }

    #[tokio::test]
    async fn broadcast_stores_single_message() {
        let store = Arc::new(MockStore::default());
        let service = MessageService::new(store);
        let sender = AgentId::new();
        let result = service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: sender,
                to: MessageTarget::Broadcast,
                body: "hi".into(),
                reply_to: None,
            })
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].to(), &MessageTarget::Broadcast);
    }

    #[tokio::test]
    async fn mark_read_uses_receipt_for_role_message() {
        let store = Arc::new(MockStore::default());
        let sender = AgentId::new();
        let receiver = AgentId::new();

        let service = MessageService::new(Arc::clone(&store));
        let sent = service
            .send(SendMessage {
                org_id: test_org(),
                project: test_project(),
                namespace: Namespace::root(),
                from: sender,
                to: MessageTarget::Role("reviewer".to_string()),
                body: "review please".into(),
                reply_to: None,
            })
            .await
            .unwrap();

        service.mark_read(&receiver, &[sent[0].id()]).await.unwrap();

        let pending = service
            .pending(
                &receiver,
                &["reviewer".to_string()],
                &test_org(),
                &test_project(),
                &Namespace::root(),
                PageParams::unbounded(),
            )
            .await
            .unwrap();
        assert!(pending.items.is_empty());
    }
}
