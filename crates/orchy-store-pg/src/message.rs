use async_trait::async_trait;
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
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::ResourceRef;
use orchy_events::io::Writer;

use crate::{PgBackend, events::PgEventWriter, parse_namespace, parse_project_id};

#[derive(Iden)]
enum Messages {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "organization_id"]
    OrganizationId,
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
    #[iden = "refs"]
    Refs,
}

#[async_trait]
impl MessageStore for PgBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO messages (id, organization_id, project, namespace, from_agent, to_target, body, reply_to, status, created_at, refs)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (id) DO UPDATE SET
                organization_id = EXCLUDED.organization_id,
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                from_agent = EXCLUDED.from_agent,
                to_target = EXCLUDED.to_target,
                body = EXCLUDED.body,
                reply_to = EXCLUDED.reply_to,
                status = EXCLUDED.status,
                refs = EXCLUDED.refs",
        )
        .bind(message.id().as_uuid())
        .bind(message.org_id().to_string())
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
        .bind(serde_json::to_value(message.refs()).unwrap_or(serde_json::json!([])))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = message.drain_events();
        PgEventWriter::new_tx(&mut tx)
            .write_all(&events)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let row = sqlx::query(
            "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs
             FROM messages WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_message(&r)).transpose()
    }

    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<Message>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let uuid_ids: Vec<Uuid> = ids.iter().map(|id| *id.as_uuid()).collect();
        let rows = sqlx::query(
            "SELECT id, organization_id, project, namespace, from_agent, to_target, body, \
             status, created_at, reply_to \
             FROM messages WHERE id = ANY($1::uuid[])",
        )
        .bind(&uuid_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        rows.iter().map(row_to_message).collect()
    }

    async fn mark_read(&self, agent: &AgentId, message_ids: &[MessageId]) -> Result<()> {
        for id in message_ids {
            sqlx::query(
                "INSERT INTO message_receipts (message_id, agent_id, read_at)
                 VALUES ($1, $2, NOW())
                 ON CONFLICT (message_id, agent_id) DO UPDATE SET read_at = EXCLUDED.read_at",
            )
            .bind(id.as_uuid())
            .bind(agent.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }
        Ok(())
    }

    async fn find_unread(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        agent_namespace: &Namespace,
        org: &OrganizationId,
        project: &ProjectId,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let fetch_limit = (page.limit as i64).saturating_add(1);
        let cursor_id: Option<Uuid> = page
            .after
            .as_ref()
            .and_then(|c| decode_cursor(c))
            .and_then(|s| s.parse::<Uuid>().ok());
        let role_set: Vec<String> = agent_roles.iter().map(|r| format!("role:{r}")).collect();
        let ns_str = agent_namespace.to_string();

        let rows = if let Some(cid) = cursor_id {
            sqlx::query(
                "SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs
                 FROM messages m
                 LEFT JOIN message_receipts r ON r.message_id = m.id AND r.agent_id = $1
                 WHERE r.message_id IS NULL
                   AND m.organization_id = $2
                   AND m.project = $3
                   AND m.id < $4
                   AND (
                        m.to_target = $1
                        OR (m.to_target = 'broadcast' AND m.from_agent != $1)
                        OR (m.to_target LIKE 'role:%' AND m.from_agent != $1)
                        OR (m.to_target LIKE 'ns:%' AND m.from_agent != $1)
                   )
                 ORDER BY m.id DESC LIMIT $5",
            )
            .bind(agent.to_string())
            .bind(org.to_string())
            .bind(project.to_string())
            .bind(cid)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs
                 FROM messages m
                 LEFT JOIN message_receipts r ON r.message_id = m.id AND r.agent_id = $1
                 WHERE r.message_id IS NULL
                   AND m.organization_id = $2
                   AND m.project = $3
                   AND (
                        m.to_target = $1
                        OR (m.to_target = 'broadcast' AND m.from_agent != $1)
                        OR (m.to_target LIKE 'role:%' AND m.from_agent != $1)
                        OR (m.to_target LIKE 'ns:%' AND m.from_agent != $1)
                   )
                 ORDER BY m.id DESC LIMIT $4",
            )
            .bind(agent.to_string())
            .bind(org.to_string())
            .bind(project.to_string())
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        };

        let mut messages: Vec<Message> = rows
            .iter()
            .map(row_to_message)
            .collect::<Result<Vec<_>>>()?;

        // App-layer filtering: role match + namespace hierarchy
        messages.retain(|m| match m.to() {
            MessageTarget::Role(role) => role_set.contains(&format!("role:{role}")),
            MessageTarget::Namespace(ns) => ns_str.starts_with(&ns.to_string()),
            _ => true,
        });

        let has_more = messages.len() > page.limit as usize;
        if has_more {
            messages.truncate(page.limit as usize);
        }
        let next_cursor = if has_more {
            messages.last().map(|m| encode_cursor(&m.id().to_string()))
        } else {
            None
        };

        Ok(Page::new(messages, next_cursor))
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let mut select = Query::select();
        select
            .from(Messages::Table)
            .columns([
                Messages::Id,
                Messages::OrganizationId,
                Messages::Project,
                Messages::Namespace,
                Messages::FromAgent,
                Messages::ToTarget,
                Messages::Body,
                Messages::Status,
                Messages::CreatedAt,
                Messages::ReplyTo,
                Messages::Refs,
            ])
            .and_where(Expr::col(Messages::FromAgent).eq(*sender.as_uuid()))
            .and_where(Expr::col(Messages::OrganizationId).eq(org.to_string()))
            .and_where(Expr::col(Messages::Project).eq(project.to_string()));

        if !namespace.is_root() {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Messages::Namespace).eq(namespace.to_string()))
                    .add(Expr::col(Messages::Namespace).like(format!("{}/%", namespace))),
            );
        }

        if let Some(ref cursor) = page.after
            && let Some(decoded) = decode_cursor(cursor)
            && let Ok(cursor_uuid) = decoded.parse::<Uuid>()
        {
            select.and_where(Expr::col(Messages::Id).lt(cursor_uuid));
        }

        select
            .order_by(Messages::CreatedAt, sea_query::Order::Desc)
            .order_by(Messages::Id, sea_query::Order::Desc)
            .limit((page.limit as u64).saturating_add(1));

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut messages: Vec<Message> = rows
            .iter()
            .map(row_to_message)
            .collect::<Result<Vec<_>>>()?;
        let has_more = messages.len() > page.limit as usize;
        if has_more {
            messages.truncate(page.limit as usize);
        }
        let next_cursor = if has_more {
            messages.last().map(|m| encode_cursor(&m.id().to_string()))
        } else {
            None
        };
        Ok(Page::new(messages, next_cursor))
    }

    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        let mut sql = String::from(
            "WITH RECURSIVE
             ancestors AS (
                 SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs
                 FROM messages WHERE id = $1
                 UNION ALL
                 SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs
                 FROM messages m JOIN ancestors a ON m.id = a.reply_to
             ),
             root AS (
                 SELECT id FROM ancestors WHERE reply_to IS NULL
             ),
             thread AS (
                 SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs
                 FROM messages WHERE id = (SELECT id FROM root LIMIT 1)
                 UNION ALL
                 SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs
                 FROM messages m JOIN thread t ON m.reply_to = t.id
             )
             SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs
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
    let org_id_str: String = row.get("organization_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let from_agent: Uuid = row.get("from_agent");
    let to_target: String = row.get("to_target");
    let body: String = row.get("body");
    let status: String = row.get("status");
    let created_at: DateTime<Utc> = row.get("created_at");
    let reply_to: Option<Uuid> = row.get("reply_to");
    let refs_json: serde_json::Value = row.get("refs");
    let refs: Vec<ResourceRef> = serde_json::from_value(refs_json).unwrap_or_default();

    Ok(Message::restore(RestoreMessage {
        id: MessageId::from_uuid(id),
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid messages.organization_id: {e}")))?,
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
        refs,
    }))
}
