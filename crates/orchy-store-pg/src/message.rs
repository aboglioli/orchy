use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::PgBackend;

impl MessageStore for PgBackend {
    async fn save(&self, message: &Message) -> Result<()> {
        sqlx::query(
            "INSERT INTO messages (id, project, namespace, from_agent, to_target, body, reply_to, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO UPDATE SET
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                from_agent = EXCLUDED.from_agent,
                to_target = EXCLUDED.to_target,
                body = EXCLUDED.body,
                reply_to = EXCLUDED.reply_to,
                status = EXCLUDED.status,
                created_at = EXCLUDED.created_at",
        )
        .bind(message.id().as_uuid())
        .bind(message.project().to_string())
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
            "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
             FROM messages WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_message(&r)))
    }

    async fn find_pending(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let mut sql = String::from(
            "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
             FROM messages
             WHERE status = 'pending' AND (to_target = $1 OR to_target = 'broadcast')
               AND project = $2",
        );
        let ns_str = namespace.to_string();
        let mut bind_ns = false;

        if !namespace.is_root() {
            sql.push_str(" AND (namespace = $3 OR namespace LIKE $3 || '/%')");
            bind_ns = true;
        }

        let mut q = sqlx::query(&sql);
        q = q.bind(agent.to_string());
        q = q.bind(project.to_string());
        if bind_ns {
            q = q.bind(&ns_str);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_message).collect())
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let mut sql = String::from(
            "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
             FROM messages
             WHERE from_agent = $1 AND project = $2",
        );
        let ns_str = namespace.to_string();
        let mut bind_ns = false;

        if !namespace.is_root() {
            sql.push_str(" AND (namespace = $3 OR namespace LIKE $3 || '/%')");
            bind_ns = true;
        }

        sql.push_str(" ORDER BY created_at DESC");

        let mut q = sqlx::query(&sql);
        q = q.bind(sender.as_uuid());
        q = q.bind(project.to_string());
        if bind_ns {
            q = q.bind(&ns_str);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_message).collect())
    }

    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        let mut sql = String::from(
            "WITH RECURSIVE
             ancestors AS (
                 SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
                 FROM messages WHERE id = $1
                 UNION ALL
                 SELECT m.id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to
                 FROM messages m JOIN ancestors a ON m.id = a.reply_to
             ),
             root AS (
                 SELECT id FROM ancestors WHERE reply_to IS NULL
             ),
             thread AS (
                 SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
                 FROM messages WHERE id = (SELECT id FROM root LIMIT 1)
                 UNION ALL
                 SELECT m.id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to
                 FROM messages m JOIN thread t ON m.reply_to = t.id
             )
             SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
             FROM thread ORDER BY created_at ASC",
        );

        if let Some(n) = limit {
            sql = format!(
                "SELECT * FROM ({sql}) sub ORDER BY created_at DESC LIMIT {n}"
            );
            sql = format!(
                "SELECT * FROM ({sql}) sub2 ORDER BY created_at ASC"
            );
        }

        let rows = sqlx::query(&sql)
            .bind(message_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_message).collect())
    }
}

fn row_to_message(row: &sqlx::postgres::PgRow) -> Message {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let from_agent: Uuid = row.get("from_agent");
    let to_target: String = row.get("to_target");
    let body: String = row.get("body");
    let status: String = row.get("status");
    let created_at: DateTime<Utc> = row.get("created_at");
    let reply_to: Option<Uuid> = row.get("reply_to");

    Message::restore(
        MessageId::from_uuid(id),
        ProjectId::try_from(project).expect("invalid project in database"),
        Namespace::try_from(namespace).expect("invalid namespace in database"),
        AgentId::from_uuid(from_agent),
        MessageTarget::parse(&to_target).unwrap_or(MessageTarget::Broadcast),
        body,
        reply_to.map(MessageId::from_uuid),
        status.parse::<MessageStatus>().unwrap_or(MessageStatus::Pending),
        created_at,
    )
}