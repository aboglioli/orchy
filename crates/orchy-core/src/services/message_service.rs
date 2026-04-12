use std::sync::Arc;

use crate::entities::{CreateMessage, Message};
use crate::error::Result;
use crate::store::Store;
use crate::value_objects::{AgentId, MessageId, MessageTarget, Namespace};

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

    pub async fn check(
        &self,
        agent: &AgentId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        self.store.check_messages(agent, namespace).await
    }

    pub async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        self.store.mark_messages_read(ids).await
    }
}
