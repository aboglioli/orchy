use std::str::FromStr;

use chrono::{DateTime, Utc};

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::Namespace;

use crate::SqliteBackend;

impl MessageStore for SqliteBackend {
    async fn save(&self, message: &Message) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO messages (id, namespace, from_agent, to_target, body, reply_to, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                message.id().to_string(),
                message.namespace().to_string(),
                message.from().to_string(),
                message.to().to_string(),
                message.body(),
                message.reply_to().map(|id| id.to_string()),
                format!("{}", match message.status() {
                    MessageStatus::Pending => "pending",
                    MessageStatus::Delivered => "delivered",
                    MessageStatus::Read => "read",
                }),
                message.created_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, from_agent, to_target, body, status, created_at, reply_to
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

    async fn find_pending(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, from_agent, to_target, body, status, created_at, reply_to
                 FROM messages
                 WHERE status = 'pending' AND (to_target = ?1 OR to_target = 'broadcast')
                 AND (namespace = ?2 OR namespace LIKE ?2 || '/%')",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let messages = stmt
            .query_map(
                rusqlite::params![agent.to_string(), namespace.to_string()],
                row_to_message,
            )
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(messages)
    }
}

fn row_to_message(row: &rusqlite::Row) -> rusqlite::Result<Message> {
    let id_str: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let from_str: String = row.get(2)?;
    let to_str: String = row.get(3)?;
    let body: String = row.get(4)?;
    let status_str: String = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let reply_to_str: Option<String> = row.get(7)?;

    let reply_to = reply_to_str
        .map(|s| {
            MessageId::from_str(&s).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .transpose()?;

    Ok(Message::restore(
        MessageId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        AgentId::from_str(&from_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        MessageTarget::parse(&to_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
            )
        })?,
        body,
        reply_to,
        parse_message_status(&status_str),
        DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    ))
}

fn parse_message_status(s: &str) -> MessageStatus {
    match s {
        "pending" => MessageStatus::Pending,
        "delivered" => MessageStatus::Delivered,
        "read" => MessageStatus::Read,
        _ => MessageStatus::Pending,
    }
}
