use rusqlite::{Error as RusqliteError, Row as RusqliteRow, types::Type as RusqliteType};
use std::error::Error as StdError;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::iter::repeat_n;
use std::result::Result as StdResult;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result, StoreError};
use orchy_core::message::{
    Message, MessageId, MessageStatus, MessageStore, MessageTarget, RestoreMessage,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::ResourceRef;
use orchy_core::user::UserId;

use crate::{SqliteConn, blocking, blocking_tx, events};

fn str_err(e: impl ToString) -> Box<dyn StdError + Send + Sync> {
    Box::new(IoError::new(IoErrorKind::InvalidData, e.to_string()))
}

pub struct SqliteMessageStore {
    conn: SqliteConn,
}

impl SqliteMessageStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl MessageStore for SqliteMessageStore {
    async fn save(&self, message: &mut Message) -> Result<()> {
        let id = message.id().to_string();
        let org_id = message.org_id().to_string();
        let project = message.project().to_string();
        let namespace = message.namespace().to_string();
        let from = message.from().to_string();
        let to = message.to().to_string();
        let body = message.body().to_string();
        let reply_to = message.reply_to().map(|id| id.to_string());
        let status = match message.status() {
            MessageStatus::Pending => "pending",
            MessageStatus::Delivered => "delivered",
            MessageStatus::Read => "read",
        }
        .to_string();
        let created_at = message.created_at().to_rfc3339();
        let refs = serde_json::to_string(message.refs()).unwrap_or_else(|_| "[]".to_string());
        let claimed_by = message.claimed_by().map(|id| id.to_string());
        let claimed_at = message.claimed_at().map(|dt| dt.to_rfc3339());
        let drained = message.drain_events();
        blocking_tx(&self.conn, move |tx| {
            tx.execute(
                "INSERT OR REPLACE INTO messages (id, organization_id, project, namespace, from_agent, to_target, body, reply_to, status, created_at, refs, claimed_by, claimed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    id, org_id, project, namespace, from, to, body, reply_to, status, created_at,
                    refs, claimed_by, claimed_at,
                ],
            )
            .map_err(crate::error::store_err)?;
            events::write_events_in_tx(tx, &drained)?;
            Ok(())
        })
        .await
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let id = id.to_string();
        blocking(&self.conn, move |conn| {
            use rusqlite::OptionalExtension;
            let mut stmt = conn
                .prepare(
                    "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs, claimed_by, claimed_at
                     FROM messages WHERE id = ?1",
                )
                .map_err(crate::error::store_err)?;

            stmt.query_row(rusqlite::params![id], row_to_message)
                .optional()
                .map_err(crate::error::store_err)
        })
        .await
    }

    async fn mark_read(&self, agent: &AgentId, message_ids: &[MessageId]) -> Result<()> {
        let agent = agent.to_string();
        let ids: Vec<String> = message_ids.iter().map(|id| id.to_string()).collect();
        blocking_tx(&self.conn, move |tx| {
            let now = Utc::now().to_rfc3339();
            for id in &ids {
                tx.execute(
                    "INSERT OR REPLACE INTO message_receipts (message_id, agent_id, read_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, agent, &now],
                )
                .map_err(crate::error::store_err)?;
            }
            Ok(())
        })
        .await
    }

    async fn find_unread(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        agent_namespace: &Namespace,
        agent_user_id: Option<&UserId>,
        org: &OrganizationId,
        project: &ProjectId,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let agent = agent.to_string();
        let org = org.to_string();
        let project = project.to_string();

        let role_targets: Vec<String> = agent_roles.iter().map(|r| format!("role:{r}")).collect();
        let role_targets_json = serde_json::to_string(&role_targets)
            .map_err(|e| Error::Store(StoreError::Serialization(format!("role targets: {e}"))))?;

        let ns_targets = namespace_ancestors(agent_namespace);
        let ns_targets_json = serde_json::to_string(&ns_targets)
            .map_err(|e| Error::Store(StoreError::Serialization(format!("ns targets: {e}"))))?;

        let user_targets: Vec<String> = agent_user_id
            .map(|uid| vec![format!("user:{uid}")])
            .unwrap_or_default();
        let user_targets_json = serde_json::to_string(&user_targets)
            .map_err(|e| Error::Store(StoreError::Serialization(format!("user targets: {e}"))))?;

        blocking(&self.conn, move |conn| {
            let mut sql = String::from(
                "SELECT m.id, m.organization_id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to, m.refs, m.claimed_by, m.claimed_at
                 FROM messages m
                 LEFT JOIN message_receipts r ON r.message_id = m.id AND r.agent_id = ?1
                 WHERE r.message_id IS NULL
                   AND m.organization_id = ?2
                   AND m.project = ?3
                   AND (
                        m.to_target = ?1
                        OR (m.to_target = 'broadcast' AND m.from_agent != ?1)
                        OR (m.to_target IN (SELECT value FROM json_each(?4)) AND m.from_agent != ?1)
                        OR (m.to_target IN (SELECT value FROM json_each(?5)) AND m.from_agent != ?1)
                        OR (m.to_target IN (SELECT value FROM json_each(?6)) AND m.from_agent != ?1)
                   )
                   AND (m.claimed_by IS NULL OR m.claimed_by = ?1)",
            );

            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
                Box::new(agent),
                Box::new(org),
                Box::new(project),
                Box::new(role_targets_json),
                Box::new(ns_targets_json),
                Box::new(user_targets_json),
            ];

            let mut idx = 7;

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

            let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let mut messages: Vec<Message> = stmt
                .query_map(param_refs.as_slice(), row_to_message)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)?;

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
        })
        .await
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let sender = sender.to_string();
        let org = org.to_string();
        let project = project.to_string();
        let namespace = namespace.clone();
        blocking(&self.conn, move |conn| {
            let mut sql = String::from(
                "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs FROM messages WHERE from_agent = ?1 AND organization_id = ?2 AND project = ?3",
            );
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
                Box::new(sender),
                Box::new(org),
                Box::new(project),
            ];
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

            let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let mut messages: Vec<Message> = stmt
                .query_map(param_refs.as_slice(), row_to_message)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)?;

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
        })
        .await
    }

    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        let message_id = message_id.to_string();
        blocking(&self.conn, move |conn| {
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

            let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;

            stmt.query_map(rusqlite::params![message_id], row_to_message)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)
        })
        .await
    }

    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<Message>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        blocking(&self.conn, move |conn| {
            let placeholders: String = repeat_n("?", id_strings.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT id, organization_id, project, namespace, from_agent, to_target, body, status, created_at, reply_to, refs, claimed_by, claimed_at \
                 FROM messages WHERE id IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = id_strings
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
            stmt.query_map(param_refs.as_slice(), row_to_message)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)
        })
        .await
    }
}

fn namespace_ancestors(ns: &Namespace) -> Vec<String> {
    let mut ancestors = vec![format!("ns:{}", Namespace::root())];
    let mut current = ns.clone();
    while !current.is_root() {
        ancestors.push(format!("ns:{current}"));
        current = current.parent();
    }
    ancestors.dedup();
    ancestors
}

fn row_to_message(row: &RusqliteRow) -> rusqlite::Result<Message> {
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
    let claimed_by_str: Option<String> = row.get(11).ok();
    let claimed_at_str: Option<String> = row.get(12).ok();

    let reply_to = reply_to_str
        .map(|s| {
            MessageId::from_str(&s).map_err(|e| {
                RusqliteError::FromSqlConversionFailure(9, RusqliteType::Text, str_err(e))
            })
        })
        .transpose()?;

    let refs: Vec<ResourceRef> = serde_json::from_str(&refs_str).unwrap_or_default();

    let claimed_by = claimed_by_str.and_then(|s| AgentId::from_str(&s).ok());
    let claimed_at = claimed_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    Ok(Message::restore(RestoreMessage {
        id: MessageId::from_str(&id_str).map_err(|e| {
            RusqliteError::FromSqlConversionFailure(0, RusqliteType::Text, str_err(e))
        })?,
        org_id: OrganizationId::new(&org_id_str).map_err(|e| {
            RusqliteError::FromSqlConversionFailure(
                1,
                RusqliteType::Text,
                Box::new(IoError::new(IoErrorKind::InvalidData, e.to_string())),
            )
        })?,
        project: ProjectId::try_from(project_str).map_err(|e| {
            RusqliteError::FromSqlConversionFailure(
                2,
                RusqliteType::Text,
                Box::new(IoError::new(IoErrorKind::InvalidData, e)),
            )
        })?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            RusqliteError::FromSqlConversionFailure(
                3,
                RusqliteType::Text,
                Box::new(IoError::new(IoErrorKind::InvalidData, e)),
            )
        })?,
        from: AgentId::from_str(&from_str).map_err(|e| {
            RusqliteError::FromSqlConversionFailure(4, RusqliteType::Text, str_err(e))
        })?,
        to: MessageTarget::parse(&to_str).map_err(|e| {
            RusqliteError::FromSqlConversionFailure(
                5,
                RusqliteType::Text,
                Box::new(IoError::new(IoErrorKind::InvalidData, e.to_string())),
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
                RusqliteError::FromSqlConversionFailure(8, RusqliteType::Text, Box::new(e))
            })?,
        refs,
        claimed_by,
        claimed_at,
    }))
}
