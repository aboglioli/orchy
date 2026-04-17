use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{
    RestoreReviewRequest, ReviewId, ReviewRequest, ReviewStatus, ReviewStore, TaskId,
};

use crate::SqliteBackend;

fn str_err(e: impl ToString) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}

#[async_trait]
impl ReviewStore for SqliteBackend {
    async fn save(&self, review: &mut ReviewRequest) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO reviews (id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                review.id().to_string(),
                review.org_id().to_string(),
                review.task_id().to_string(),
                review.project().to_string(),
                review.namespace().to_string(),
                review.requester().to_string(),
                review.reviewer().map(|a| a.to_string()),
                review.reviewer_role().map(|s| s.to_string()),
                review.status().to_string(),
                review.comments().map(|s| s.to_string()),
                review.created_at().to_rfc3339(),
                review.resolved_at().map(|dt| dt.to_rfc3339()),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = review.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ReviewId) -> Result<Option<ReviewRequest>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
                 FROM reviews WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_review)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn find_pending_for_agent(&self, agent_id: &AgentId) -> Result<Vec<ReviewRequest>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
                 FROM reviews WHERE reviewer = ?1 AND status = 'pending'",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let reviews = stmt
            .query_map(rusqlite::params![agent_id.to_string()], row_to_review)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(reviews)
    }

    async fn find_by_task(&self, task_id: &TaskId) -> Result<Vec<ReviewRequest>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
                 FROM reviews WHERE task_id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let reviews = stmt
            .query_map(rusqlite::params![task_id.to_string()], row_to_review)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(reviews)
    }
}

fn row_to_review(row: &rusqlite::Row) -> rusqlite::Result<ReviewRequest> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let task_id_str: String = row.get(2)?;
    let project_str: String = row.get(3)?;
    let namespace_str: String = row.get(4)?;
    let requester_str: String = row.get(5)?;
    let reviewer_str: Option<String> = row.get(6)?;
    let reviewer_role: Option<String> = row.get(7)?;
    let status_str: String = row.get(8)?;
    let comments: Option<String> = row.get(9)?;
    let created_at_str: String = row.get(10)?;
    let resolved_at_str: Option<String> = row.get(11)?;

    let id = ReviewId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, str_err(e))
    })?;
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, str_err(e))
    })?;
    let task_id = TaskId::from_str(&task_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, str_err(e))
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, str_err(e))
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, str_err(e))
    })?;
    let requester = AgentId::from_str(&requester_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, str_err(e))
    })?;
    let reviewer = reviewer_str.and_then(|s| AgentId::from_str(&s).ok());
    let status = status_str
        .parse::<ReviewStatus>()
        .unwrap_or(ReviewStatus::Pending);
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, str_err(e))
        })?;
    let resolved_at = resolved_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    Ok(ReviewRequest::restore(RestoreReviewRequest {
        id,
        org_id,
        task_id,
        project,
        namespace,
        requester,
        reviewer,
        reviewer_role,
        status,
        comments,
        created_at,
        resolved_at,
    }))
}
