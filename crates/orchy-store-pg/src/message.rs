use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{
    CreateMessage, Message, MessageId, MessageStatus, MessageStore, MessageTarget,
};
use orchy_core::namespace::Namespace;

use crate::PgBackend;

impl MessageStore for PgBackend {
    async fn send(&self, cmd: CreateMessage) -> Result<Message> {
        let message = Message {
            id: MessageId::new(),
            namespace: cmd.namespace,
            from: cmd.from,
            to: cmd.to,
            body: cmd.body,
            status: MessageStatus::Pending,
            created_at: Utc::now(),
        };

        sqlx::query(
            "INSERT INTO messages (id, namespace, from_agent, to_target, body, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(message.id.as_uuid())
        .bind(message.namespace.to_string())
        .bind(message.from.as_uuid())
        .bind(message.to.to_string())
        .bind(&message.body)
        .bind("pending")
        .bind(message.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(message)
    }

    async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            "SELECT id, namespace, from_agent, to_target, body, status, created_at
             FROM messages
             WHERE status = 'pending' AND (to_target = $1 OR to_target = 'broadcast')
               AND (namespace = $2 OR namespace LIKE $2 || '/%')",
        )
        .bind(agent.to_string())
        .bind(namespace.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let messages: Vec<Message> = rows.iter().map(row_to_message).collect();

        // Mark as delivered
        for msg in &messages {
            sqlx::query("UPDATE messages SET status = 'delivered' WHERE id = $1")
                .bind(msg.id.as_uuid())
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;
        }

        // Return with delivered status
        let delivered: Vec<Message> = messages
            .into_iter()
            .map(|mut m| {
                m.status = MessageStatus::Delivered;
                m
            })
            .collect();

        Ok(delivered)
    }

    async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        for id in ids {
            sqlx::query("UPDATE messages SET status = 'read' WHERE id = $1")
                .bind(id.as_uuid())
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;
        }
        Ok(())
    }
}

fn row_to_message(row: &sqlx::postgres::PgRow) -> Message {
    let id: Uuid = row.get("id");
    let namespace: String = row.get("namespace");
    let from_agent: Uuid = row.get("from_agent");
    let to_target: String = row.get("to_target");
    let body: String = row.get("body");
    let status: String = row.get("status");
    let created_at: DateTime<Utc> = row.get("created_at");

    Message {
        id: MessageId::from_uuid(id),
        namespace: Namespace::try_from(namespace).expect("invalid namespace in database"),
        from: AgentId::from_uuid(from_agent),
        to: MessageTarget::parse(&to_target).unwrap_or(MessageTarget::Broadcast),
        body,
        status: parse_message_status(&status),
        created_at,
    }
}

fn parse_message_status(s: &str) -> MessageStatus {
    match s {
        "pending" => MessageStatus::Pending,
        "delivered" => MessageStatus::Delivered,
        "read" => MessageStatus::Read,
        _ => MessageStatus::Pending,
    }
}
