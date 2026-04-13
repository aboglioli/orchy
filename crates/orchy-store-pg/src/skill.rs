use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::skill::{RestoreSkill, Skill, SkillFilter, SkillStore};

use crate::PgBackend;

impl SkillStore for PgBackend {
    async fn save(&self, skill: &Skill) -> Result<()> {
        sqlx::query(
            "INSERT INTO skills (project, namespace, name, description, content, written_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (project, namespace, name) DO UPDATE
             SET description = EXCLUDED.description,
                 content = EXCLUDED.content,
                 written_by = EXCLUDED.written_by,
                 updated_at = EXCLUDED.updated_at",
        )
        .bind(skill.project().to_string())
        .bind(skill.namespace().to_string())
        .bind(skill.name())
        .bind(skill.description())
        .bind(skill.content())
        .bind(skill.written_by().map(|a| *a.as_uuid()))
        .bind(skill.created_at())
        .bind(skill.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_name(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<Skill>> {
        let row = sqlx::query(
            "SELECT project, namespace, name, description, content, written_by, created_at, updated_at
             FROM skills WHERE project = $1 AND namespace = $2 AND name = $3",
        )
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_skill(&r)))
    }

    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        let mut sql = "SELECT project, namespace, name, description, content, written_by, created_at, updated_at FROM skills WHERE 1=1".to_string();
        let mut params: Vec<String> = Vec::new();
        let mut idx = 1u32;

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                sql.push_str(&format!(
                    " AND (namespace = ${idx} OR namespace LIKE ${idx} || '/%')"
                ));
                params.push(ns.to_string());
                idx += 1;
            }
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(" AND project = ${idx}"));
            params.push(project.to_string());
        }

        let mut query = sqlx::query(&sql);
        for p in &params {
            query = query.bind(p);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_skill).collect())
    }

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        sqlx::query("DELETE FROM skills WHERE project = $1 AND namespace = $2 AND name = $3")
            .bind(project.to_string())
            .bind(namespace.to_string())
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_skill(row: &sqlx::postgres::PgRow) -> Skill {
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let name: String = row.get("name");
    let description: String = row.get("description");
    let content: String = row.get("content");
    let written_by: Option<Uuid> = row.get("written_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    Skill::restore(RestoreSkill {
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).unwrap(),
        name,
        description,
        content,
        written_by: written_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    })
}
