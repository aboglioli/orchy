use chrono::{DateTime, Utc};
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{
    Message, MessageId, MessageStatus, MessageStore, MessageTarget, RestoreMessage,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::{PgBackend, parse_namespace, parse_project_id};

#[derive(Iden)]
enum Messages {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "from_agent"]
    FromAgent,
    #[iden = "to_target"]
    ToTarget,
    #[iden = "body"]
    Body,
    #[iden = "status"]
    Status,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "reply_to"]
    ReplyTo,
}

impl MessageStore for PgBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
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
                status = EXCLUDED.status",
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

        let events = message.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

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

        row.map(|r| row_to_message(&r)).transpose()
    }

    async fn mark_read_for_agent(&self, message_id: &MessageId, agent: &AgentId) -> Result<()> {
        sqlx::query(
            "INSERT INTO message_receipts (message_id, agent_id, read_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (message_id, agent_id) DO UPDATE SET read_at = EXCLUDED.read_at",
        )
        .bind(message_id.as_uuid())
        .bind(agent.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_pending(
        &self,
        agent: &AgentId,
        _org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let rows = if namespace.is_root() {
            sqlx::query(
                "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
                 FROM messages
                 WHERE status = 'pending'
                   AND project = $1
                   AND (
                        to_target = $2
                        OR (
                            to_target = 'broadcast'
                            AND from_agent != $3
                            AND NOT EXISTS (
                                SELECT 1 FROM message_receipts
                                WHERE message_receipts.message_id = messages.id
                                  AND message_receipts.agent_id = $3
                            )
                        )
                   )",
            )
            .bind(project.to_string())
            .bind(agent.to_string())
            .bind(agent.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
                 FROM messages
                 WHERE status = 'pending'
                   AND project = $1
                   AND (namespace = $2 OR namespace LIKE $2 || '/%')
                   AND (
                        to_target = $3
                        OR (
                            to_target = 'broadcast'
                            AND from_agent != $4
                            AND NOT EXISTS (
                                SELECT 1 FROM message_receipts
                                WHERE message_receipts.message_id = messages.id
                                  AND message_receipts.agent_id = $4
                            )
                        )
                   )",
            )
            .bind(project.to_string())
            .bind(namespace.to_string())
            .bind(agent.to_string())
            .bind(agent.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        };

        rows.iter().map(row_to_message).collect()
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        _org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let mut select = Query::select();
        select
            .from(Messages::Table)
            .columns([
                Messages::Id,
                Messages::Project,
                Messages::Namespace,
                Messages::FromAgent,
                Messages::ToTarget,
                Messages::Body,
                Messages::Status,
                Messages::CreatedAt,
                Messages::ReplyTo,
            ])
            .and_where(Expr::col(Messages::FromAgent).eq(*sender.as_uuid()))
            .and_where(Expr::col(Messages::Project).eq(project.to_string()));

        if !namespace.is_root() {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Messages::Namespace).eq(namespace.to_string()))
                    .add(Expr::col(Messages::Namespace).like(format!("{}/%", namespace))),
            );
        }

        select.order_by(Messages::CreatedAt, sea_query::Order::Desc);

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_message).collect()
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
            sql = format!("SELECT * FROM ({sql}) sub ORDER BY created_at DESC LIMIT {n}");
            sql = format!("SELECT * FROM ({sql}) sub2 ORDER BY created_at ASC");
        }

        let rows = sqlx::query(&sql)
            .bind(message_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_message).collect()
    }
}

fn row_to_message(row: &sqlx::postgres::PgRow) -> Result<Message> {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let from_agent: Uuid = row.get("from_agent");
    let to_target: String = row.get("to_target");
    let body: String = row.get("body");
    let status: String = row.get("status");
    let created_at: DateTime<Utc> = row.get("created_at");
    let reply_to: Option<Uuid> = row.get("reply_to");

    Ok(Message::restore(RestoreMessage {
        id: MessageId::from_uuid(id),
        org_id: OrganizationId::new("default").unwrap(),
        project: parse_project_id(project, "messages", "project")?,
        namespace: parse_namespace(namespace, "messages", "namespace")?,
        from: AgentId::from_uuid(from_agent),
        to: MessageTarget::parse(&to_target).unwrap_or(MessageTarget::Broadcast),
        body,
        reply_to: reply_to.map(MessageId::from_uuid),
        status: status
            .parse::<MessageStatus>()
            .unwrap_or(MessageStatus::Pending),
        created_at,
    }))
}
