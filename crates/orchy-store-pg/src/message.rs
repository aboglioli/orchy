use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::Namespace;

use crate::PgBackend;

impl MessageStore for PgBackend {
    async fn save(&self, message: &Message) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, namespace, from_agent, to_target, body, reply_to, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
                namespace = EXCLUDED.namespace,
                from_agent = EXCLUDED.from_agent,
                to_target = EXCLUDED.to_target,
                body = EXCLUDED.body,
                reply_to = EXCLUDED.reply_to,
                status = EXCLUDED.status,
                created_at = EXCLUDED.created_at",
        )
        .bind(message.id().as_uuid())
        .bind(message.namespace().to_string())
        .bind(message.from().as_uuid())
        .bind(message.to().to_string())
        .bind(message.body())
        .bind(message.reply_to().map(|id| *id.as_uuid()))
        .bind(match message.status() {
            MessageStatus::Pending => "pending",
            MessageStatus::Delivered => "delivered",
            MessageStatus::Read => "read",
        })
        .bind(message.created_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let row = sqlx::query(
            "SELECT id, namespace, from_agent, to_target, body, status, created_at, reply_to
             FROM messages WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_message(&r)))
    }

    async fn find_pending(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        let rows = sqlx::query(
            "SELECT id, namespace, from_agent, to_target, body, status, created_at, reply_to
             FROM messages
             WHERE status = 'pending' AND (to_target = $1 OR to_target = 'broadcast')
               AND (namespace = $2 OR namespace LIKE $2 || '/%')",
        )
        .bind(agent.to_string())
        .bind(namespace.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_message).collect())
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
    let reply_to: Option<Uuid> = row.get("reply_to");

    Message::restore(
        MessageId::from_uuid(id),
        Namespace::try_from(namespace).expect("invalid namespace in database"),
        AgentId::from_uuid(from_agent),
        MessageTarget::parse(&to_target).unwrap_or(MessageTarget::Broadcast),
        body,
        reply_to.map(MessageId::from_uuid),
        parse_message_status(&status),
        created_at,
    )
}

fn parse_message_status(s: &str) -> MessageStatus {
    match s {
        "pending" => MessageStatus::Pending,
        "delivered" => MessageStatus::Delivered,
        "read" => MessageStatus::Read,
        _ => MessageStatus::Pending,
    }
}
