use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::task::{
    RestoreReviewRequest, ReviewId, ReviewRequest, ReviewStatus, ReviewStore, TaskId,
};

use crate::PgBackend;

impl ReviewStore for PgBackend {
    async fn save(&self, review: &ReviewRequest) -> Result<()> {
        sqlx::query(
            "INSERT INTO reviews (id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (id) DO UPDATE SET
                task_id = EXCLUDED.task_id,
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                requester = EXCLUDED.requester,
                reviewer = EXCLUDED.reviewer,
                reviewer_role = EXCLUDED.reviewer_role,
                status = EXCLUDED.status,
                comments = EXCLUDED.comments,
                resolved_at = EXCLUDED.resolved_at",
        )
        .bind(review.id().as_uuid())
        .bind(review.task_id().as_uuid())
        .bind(review.project().to_string())
        .bind(review.namespace().to_string())
        .bind(review.requester().as_uuid())
        .bind(review.reviewer().map(|a| *a.as_uuid()))
        .bind(review.reviewer_role())
        .bind(review.status().to_string())
        .bind(review.comments())
        .bind(review.created_at())
        .bind(review.resolved_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &ReviewId) -> Result<Option<ReviewRequest>> {
        let row = sqlx::query(
            "SELECT id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
             FROM reviews WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_review(&r)))
    }

    async fn find_pending_for_agent(&self, agent_id: &AgentId) -> Result<Vec<ReviewRequest>> {
        let rows = sqlx::query(
            "SELECT id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
             FROM reviews WHERE reviewer = $1 AND status = 'pending'",
        )
        .bind(agent_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_review).collect())
    }

    async fn find_by_task(&self, task_id: &TaskId) -> Result<Vec<ReviewRequest>> {
        let rows = sqlx::query(
            "SELECT id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
             FROM reviews WHERE task_id = $1",
        )
        .bind(task_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_review).collect())
    }
}

fn row_to_review(row: &sqlx::postgres::PgRow) -> ReviewRequest {
    let id: Uuid = row.get("id");
    let task_id: Uuid = row.get("task_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let requester: Uuid = row.get("requester");
    let reviewer: Option<Uuid> = row.get("reviewer");
    let reviewer_role: Option<String> = row.get("reviewer_role");
    let status: String = row.get("status");
    let comments: Option<String> = row.get("comments");
    let created_at: DateTime<Utc> = row.get("created_at");
    let resolved_at: Option<DateTime<Utc>> = row.get("resolved_at");

    ReviewRequest::restore(RestoreReviewRequest {
        id: ReviewId::from_uuid(id),
        task_id: TaskId::from_uuid(task_id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).unwrap(),
        requester: AgentId::from_uuid(requester),
        reviewer: reviewer.map(AgentId::from_uuid),
        reviewer_role,
        status: status.parse::<ReviewStatus>().unwrap_or(ReviewStatus::Pending),
        comments,
        created_at,
        resolved_at,
    })
}
