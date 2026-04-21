use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{
    Message, MessageId, MessageStatus, MessageStore, MessageTarget, RestoreMessage,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::ResourceRef;

use crate::SqliteBackend;

fn str_err(e: impl ToString) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}

#[async_trait]
impl MessageStore for SqliteBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO messages (id, organization_id, project, namespace, from_agent, to_target, body, reply_to, status, created_at, refs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                message.id().to_string(),
                message.org_id().to_string(),
                message.project().to_string(),
                message.namespace().to_string(),
                message.from().to_string(),
                message.to().to_string(),
                message.body(),
                message.reply_to().map(|id| id.to_string()),
                match message.status() {
                    MessageStatus::Pending => "pending",
                    MessageStatus::Delivered => "delivered",
                    MessageStatus::Read => "read",
                },
                message.created_at().to_rfc3339(),
                serde_json::to_string(message.refs()).unwrap_or_else(|_| "[]".to_string()),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = message.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs
                 FROM messages WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        use rusqlite::OptionalExtension;
        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_message)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn mark_read(&self, agent: &AgentId, message_ids: &[MessageId]) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn.transaction().map_err(|e| Error::Store(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        for id in message_ids {
            tx.execute(
                "INSERT OR REPLACE INTO message_receipts (message_id, agent_id, read_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![id.to_string(), agent.to_string(), &now],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }
        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_unread(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        org: &OrganizationId,
        project: &ProjectId,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs
             FROM messages m
             LEFT JOIN message_receipts r ON r.message_id = m.id AND r.agent_id = ?2
             WHERE m.status = ?1
               AND m.organization_id = ?3
               AND m.project = ?4
               AND (
                    m.to_target = ?2
                    OR (
                        m.to_target = 'broadcast'
                        AND m.from_agent != ?2
                        AND r.message_id IS NULL
                    )
                    OR (
                        m.to_target LIKE 'role:%'
                        AND m.from_agent != ?2
                        AND r.message_id IS NULL
                    )
               )",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new("pending".to_string()),
            Box::new(agent.to_string()),
            Box::new(org.to_string()),
            Box::new(project.to_string()),
        ];
        let role_set: Vec<String> = agent_roles.iter().map(|r| format!("role:{r}")).collect();

        let mut idx = 5;

        if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                sql.push_str(&format!(" AND m.id < ?{idx}"));
                params.push(Box::new(decoded));
                idx += 1;
            }
        }

        let _ = idx;
        sql.push_str(" ORDER BY m.id DESC");
        let fetch_limit = (page.limit as u64).saturating_add(1);
        sql.push_str(&format!(" LIMIT {fetch_limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut messages: Vec<Message> = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        messages.retain(|m| match m.to() {
            MessageTarget::Role(role) => role_set.contains(&format!("role:{role}")),
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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs FROM messages WHERE from_agent = ?1 AND organization_id = ?2 AND project = ?3",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(sender.to_string()));
        params.push(Box::new(org.to_string()));
        params.push(Box::new(project.to_string()));
        let mut idx = 4;

        if !namespace.is_root() {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(namespace.to_string()));
            idx += 1;
        }

        if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                sql.push_str(&format!(" AND id < ?{idx}"));
                params.push(Box::new(decoded));
                idx += 1;
            }
        }

        let _ = idx;
        sql.push_str(" ORDER BY created_at DESC, id DESC");
        let fetch_limit = (page.limit as u64).saturating_add(1);
        sql.push_str(&format!(" LIMIT {fetch_limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut messages: Vec<Message> = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "WITH RECURSIVE
             ancestors AS (
                 SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs
                 FROM messages WHERE id = ?1
                 UNION ALL
                 SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs
                 FROM messages m JOIN ancestors a ON m.id = a.reply_to
             ),
             root AS (
                 SELECT id FROM ancestors WHERE reply_to IS NULL
                 UNION
                 SELECT a.id FROM ancestors a WHERE NOT EXISTS (SELECT 1 FROM messages m2 WHERE m2.id = a.reply_to)
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

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;

        let messages = stmt
            .query_map(rusqlite::params![message_id.to_string()], row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(messages)
    }

    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<Message>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: String = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs \
             FROM messages WHERE id IN ({placeholders})"
        );
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> = id_strings
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        let messages = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(messages)
    }
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let project_str: String = row.get(2)?;
    let namespace_str: String = row.get(3)?;
    let from_str: String = row.get(4)?;
    let to_str: String = row.get(5)?;
    let body: String = row.get(6)?;
    let status_str: String = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let reply_to_str: Option<String> = row.get(9)?;
    let refs_str: String = row.get(10)?;

    let reply_to = reply_to_str
        .map(|s| {
            MessageId::from_str(&s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    str_err(e),
                )
            })
        })
        .transpose()?;

    let refs: Vec<ResourceRef> = serde_json::from_str(&refs_str).unwrap_or_default();

    Ok(Message::restore(RestoreMessage {
        id: MessageId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, str_err(e))
        })?,
        org_id: OrganizationId::new(&org_id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
            )
        })?,
        project: ProjectId::try_from(project_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        from: AgentId::from_str(&from_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, str_err(e))
        })?,
        to: MessageTarget::parse(&to_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
            )
        })?,
        body,
        reply_to,
        status: status_str
            .parse::<MessageStatus>()
            .unwrap_or(MessageStatus::Pending),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        refs,
    }))
}
