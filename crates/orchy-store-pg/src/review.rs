use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{
    RestoreReviewRequest, ReviewId, ReviewRequest, ReviewStatus, ReviewStore, TaskId,
};

use crate::{PgBackend, parse_namespace, parse_project_id};

impl ReviewStore for PgBackend {
    async fn save(&self, review: &mut ReviewRequest) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO reviews (id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
                organization_id = EXCLUDED.organization_id,
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
        .bind(review.org_id().to_string())
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
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = review.drain_events();
        crate::write_events_in_tx(&mut tx, &events).await?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ReviewId) -> Result<Option<ReviewRequest>> {
        let row = sqlx::query(
            "SELECT id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
             FROM reviews WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_review(&r)).transpose()
    }

    async fn find_pending_for_agent(&self, agent_id: &AgentId) -> Result<Vec<ReviewRequest>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
             FROM reviews WHERE reviewer = $1 AND status = 'pending'",
        )
        .bind(agent_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_review).collect()
    }

    async fn find_by_task(&self, task_id: &TaskId) -> Result<Vec<ReviewRequest>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, task_id, project, namespace, requester, reviewer, reviewer_role, status, comments, created_at, resolved_at
             FROM reviews WHERE task_id = $1",
        )
        .bind(task_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_review).collect()
    }
}

fn row_to_review(row: &sqlx::postgres::PgRow) -> Result<ReviewRequest> {
    let id: Uuid = row.get("id");
    let org_id_str: String = row.get("organization_id");
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

    Ok(ReviewRequest::restore(RestoreReviewRequest {
        id: ReviewId::from_uuid(id),
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid reviews.organization_id: {e}")))?,
        task_id: TaskId::from_uuid(task_id),
        project: parse_project_id(project, "reviews", "project")?,
        namespace: parse_namespace(namespace, "reviews", "namespace")?,
        requester: AgentId::from_uuid(requester),
        reviewer: reviewer.map(AgentId::from_uuid),
        reviewer_role,
        status: status
            .parse::<ReviewStatus>()
            .unwrap_or(ReviewStatus::Pending),
        comments,
        created_at,
        resolved_at,
    }))
}
