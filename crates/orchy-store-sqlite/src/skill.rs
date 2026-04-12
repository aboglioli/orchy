use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::Namespace;
use orchy_core::skill::{Skill, SkillFilter, SkillStore, WriteSkill};

use crate::SqliteBackend;

impl SkillStore for SqliteBackend {
    async fn write(&self, cmd: WriteSkill) -> Result<Skill> {
        let now = Utc::now();
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let existing_created_at: Option<String> = conn
            .query_row(
                "SELECT created_at FROM skills WHERE namespace = ?1 AND name = ?2",
                rusqlite::params![cmd.namespace.to_string(), cmd.name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        let created_at = existing_created_at
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);

        conn.execute(
            "INSERT OR REPLACE INTO skills (namespace, name, description, content, written_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                cmd.namespace.to_string(),
                cmd.name,
                cmd.description,
                cmd.content,
                cmd.written_by.as_ref().map(|a| a.to_string()),
                created_at.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(Skill {
            namespace: cmd.namespace,
            name: cmd.name,
            description: cmd.description,
            content: cmd.content,
            written_by: cmd.written_by,
            created_at,
            updated_at: now,
        })
    }

    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT namespace, name, description, content, written_by, created_at, updated_at
                 FROM skills WHERE namespace = ?1 AND name = ?2",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![namespace.to_string(), name], row_to_skill)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = "SELECT namespace, name, description, content, written_by, created_at, updated_at FROM skills WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(project.to_string()));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let skills = stmt
            .query_map(param_refs.as_slice(), row_to_skill)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(skills)
    }

    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "DELETE FROM skills WHERE namespace = ?1 AND name = ?2",
            rusqlite::params![namespace.to_string(), name],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_skill(row: &rusqlite::Row) -> rusqlite::Result<Skill> {
    let namespace_str: String = row.get(0)?;
    let name: String = row.get(1)?;
    let description: String = row.get(2)?;
    let content: String = row.get(3)?;
    let written_by_str: Option<String> = row.get(4)?;
    let created_at_str: String = row.get(5)?;
    let updated_at_str: String = row.get(6)?;

    Ok(Skill {
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        name,
        description,
        content,
        written_by: written_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
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
