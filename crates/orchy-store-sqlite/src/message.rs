use std::str::FromStr;

use chrono::{DateTime, Utc};

use orchy_core::entities::{CreateMessage, Message, MessageStatus};
use orchy_core::error::{Error, Result};
use orchy_core::store::MessageStore;
use orchy_core::value_objects::{AgentId, MessageId, MessageTarget, Namespace};

use crate::SqliteBackend;

impl MessageStore for SqliteBackend {
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

        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT INTO messages (id, namespace, from_agent, to_target, body, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                message.id.to_string(),
                message.namespace.to_string(),
                message.from.to_string(),
                message.to.to_string(),
                message.body,
                "pending",
                message.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(message)
    }

    async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let sql = String::from(
            "SELECT id, namespace, from_agent, to_target, body, status, created_at
             FROM messages
             WHERE status = 'pending' AND (to_target = ?1 OR to_target = 'broadcast')
             AND (namespace = ?2 OR namespace LIKE ?2 || '/%')",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(agent.to_string()));
        params.push(Box::new(namespace.to_string()));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let messages: Vec<Message> = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        // Mark them as delivered
        for msg in &messages {
            conn.execute(
                "UPDATE messages SET status = 'delivered' WHERE id = ?1",
                rusqlite::params![msg.id.to_string()],
            )
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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        for id in ids {
            conn.execute(
                "UPDATE messages SET status = 'read' WHERE id = ?1",
                rusqlite::params![id.to_string()],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }
        Ok(())
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

    Ok(Message {
        id: MessageId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        from: AgentId::from_str(&from_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        to: MessageTarget::parse(&to_str).map_err(|e| {
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
        status: parse_message_status(&status_str),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    })
}

fn parse_message_status(s: &str) -> MessageStatus {
    match s {
        "pending" => MessageStatus::Pending,
        "delivered" => MessageStatus::Delivered,
        "read" => MessageStatus::Read,
        _ => MessageStatus::Pending,
    }
}
