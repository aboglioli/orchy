use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::entities::{Skill, SkillFilter, WriteSkill};
use orchy_core::error::{Error, Result};
use orchy_core::store::SkillStore;
use orchy_core::value_objects::{AgentId, Namespace};

use crate::PgBackend;

impl SkillStore for PgBackend {
    async fn write(&self, cmd: WriteSkill) -> Result<Skill> {
        let now = Utc::now();

        let row = sqlx::query(
            "INSERT INTO skills (namespace, name, description, content, written_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (namespace, name) DO UPDATE
             SET description = EXCLUDED.description,
                 content = EXCLUDED.content,
                 written_by = EXCLUDED.written_by,
                 updated_at = EXCLUDED.updated_at
             RETURNING namespace, name, description, content, written_by, created_at, updated_at",
        )
        .bind(cmd.namespace.to_string())
        .bind(&cmd.name)
        .bind(&cmd.description)
        .bind(&cmd.content)
        .bind(cmd.written_by.map(|a| *a.as_uuid()))
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row_to_skill(&row))
    }

    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        let row = sqlx::query(
            "SELECT namespace, name, description, content, written_by, created_at, updated_at
             FROM skills WHERE namespace = $1 AND name = $2",
        )
        .bind(namespace.to_string())
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_skill(&r)))
    }

    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        let rows = if let Some(ref ns) = filter.namespace {
            sqlx::query(
                "SELECT namespace, name, description, content, written_by, created_at, updated_at
                 FROM skills WHERE namespace = $1 OR namespace LIKE $1 || '/%'",
            )
            .bind(ns.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT namespace, name, description, content, written_by, created_at, updated_at
                 FROM skills",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
        };

        Ok(rows.iter().map(row_to_skill).collect())
    }

    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM skills WHERE namespace = $1 AND name = $2")
            .bind(namespace.to_string())
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_skill(row: &sqlx::postgres::PgRow) -> Skill {
    let namespace: String = row.get("namespace");
    let written_by: Option<Uuid> = row.get("written_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    Skill {
        namespace: Namespace::try_from(namespace).unwrap(),
        name: row.get("name"),
        description: row.get("description"),
        content: row.get("content"),
        written_by: written_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    }
}
