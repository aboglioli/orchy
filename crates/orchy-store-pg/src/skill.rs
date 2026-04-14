use chrono::{DateTime, Utc};
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::skill::{RestoreSkill, Skill, SkillFilter, SkillStore};

use crate::PgBackend;

#[derive(Iden)]
enum Skills {
    Table,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "name"]
    Name,
    #[iden = "description"]
    Description,
    #[iden = "content"]
    Content,
    #[iden = "written_by"]
    WrittenBy,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

impl SkillStore for PgBackend {
    async fn save(&self, skill: &mut Skill) -> Result<()> {
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

        let events = skill.drain_events();
        for evt in &events {
            if let Ok(serialized) = orchy_events::SerializedEvent::from_event(evt) {
                let id = uuid::Uuid::parse_str(&serialized.id).unwrap();
                let _ = sqlx::query(
                    "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(id)
                .bind(&serialized.organization)
                .bind(&serialized.namespace)
                .bind(&serialized.topic)
                .bind(&serialized.payload)
                .bind(&serialized.content_type)
                .bind(serde_json::to_value(&serialized.metadata).unwrap())
                .bind(serialized.timestamp)
                .bind(serialized.version as i64)
                .execute(&self.pool)
                .await;
            }
        }

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
        let mut select = Query::select();
        select.from(Skills::Table).columns([
            Skills::Project,
            Skills::Namespace,
            Skills::Name,
            Skills::Description,
            Skills::Content,
            Skills::WrittenBy,
            Skills::CreatedAt,
            Skills::UpdatedAt,
        ]);

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                select.cond_where(
                    Cond::any()
                        .add(Expr::col(Skills::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Skills::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref project) = filter.project {
            select.and_where(Expr::col(Skills::Project).eq(project.to_string()));
        }

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
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
