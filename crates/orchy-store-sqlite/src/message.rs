use std::str::FromStr;

use chrono::{DateTime, Utc};

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{
    Message, MessageId, MessageStatus, MessageStore, MessageTarget, RestoreMessage,
};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::SqliteBackend;

impl MessageStore for SqliteBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
        {
            let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
            conn.execute(
                "INSERT OR REPLACE INTO messages (id, project, namespace, from_agent, to_target, body, reply_to, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    message.id().to_string(),
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
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = message.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
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

    async fn find_pending(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to FROM messages WHERE status = ?1 AND (to_target = ?2 OR to_target = 'broadcast') AND project = ?3",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new("pending".to_string()));
        params.push(Box::new(agent.to_string()));
        params.push(Box::new(project.to_string()));
        let mut idx = 4;

        if !namespace.is_root() {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(namespace.to_string()));
            idx += 1;
        }

        let _ = idx;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let messages = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(messages)
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to FROM messages WHERE from_agent = ?1 AND project = ?2",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(sender.to_string()));
        params.push(Box::new(project.to_string()));
        let mut idx = 3;

        if !namespace.is_root() {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(namespace.to_string()));
            idx += 1;
        }

        let _ = idx;
        sql.push_str(" ORDER BY created_at DESC");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let messages = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(messages)
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
                 SELECT id, project, namespace, from_agent, to_target, body, status, created_at, reply_to
                 FROM messages WHERE id = ?1
                 UNION ALL
                 SELECT m.id, m.project, m.namespace, m.from_agent, m.to_target, m.body, m.status, m.created_at, m.reply_to
                 FROM messages m JOIN ancestors a ON m.id = a.reply_to
             ),
             root AS (
                 SELECT id FROM ancestors WHERE reply_to IS NULL
                 UNION
                 SELECT a.id FROM ancestors a WHERE NOT EXISTS (SELECT 1 FROM messages m2 WHERE m2.id = a.reply_to)
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
            // Wrap in subquery to get the most recent N messages in chronological order
            sql = format!("SELECT * FROM ({sql}) sub ORDER BY created_at DESC LIMIT {n}");
            // Re-order chronologically
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
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    let id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let from_str: String = row.get(3)?;
    let to_str: String = row.get(4)?;
    let body: String = row.get(5)?;
    let status_str: String = row.get(6)?;
    let created_at_str: String = row.get(7)?;
    let reply_to_str: Option<String> = row.get(8)?;

    let reply_to = reply_to_str
        .map(|s| {
            MessageId::from_str(&s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .transpose()?;

    Ok(Message::restore(RestoreMessage {
        id: MessageId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        project: ProjectId::try_from(project_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        from: AgentId::from_str(&from_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?,
        to: MessageTarget::parse(&to_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
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
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    }))
}
