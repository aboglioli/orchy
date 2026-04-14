use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use sea_query::{Cond, Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::skill::{RestoreSkill, Skill, SkillFilter, SkillStore};

use crate::SqliteBackend;

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

impl SkillStore for SqliteBackend {
    async fn save(&self, skill: &Skill) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO skills (project, namespace, name, description, content, written_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                skill.project().to_string(),
                skill.namespace().to_string(),
                skill.name(),
                skill.description(),
                skill.content(),
                skill.written_by().map(|a| a.to_string()),
                skill.created_at().to_rfc3339(),
                skill.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_name(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<Skill>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT project, namespace, name, description, content, written_by, created_at, updated_at
                 FROM skills WHERE project = ?1 AND namespace = ?2 AND name = ?3",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(
                rusqlite::params![project.to_string(), namespace.to_string(), name],
                row_to_skill,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut query = Query::select();
        query.from(Skills::Table).columns([
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
                query.cond_where(
                    Cond::any()
                        .add(Expr::col(Skills::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Skills::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref project) = filter.project {
            query.and_where(Expr::col(Skills::Project).eq(project.to_string()));
        }

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let skills = stmt
            .query_map(&*values.as_params(), row_to_skill)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(skills)
    }

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "DELETE FROM skills WHERE project = ?1 AND namespace = ?2 AND name = ?3",
            rusqlite::params![project.to_string(), namespace.to_string(), name],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_skill(row: &rusqlite::Row) -> rusqlite::Result<Skill> {
    let project_str: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let name: String = row.get(2)?;
    let description: String = row.get(3)?;
    let content: String = row.get(4)?;
    let written_by_str: Option<String> = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let updated_at_str: String = row.get(7)?;

    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Skill::restore(RestoreSkill {
        project,
        namespace,
        name,
        description,
        content,
        written_by: written_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at,
        updated_at,
    }))
}
