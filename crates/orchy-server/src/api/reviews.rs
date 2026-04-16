use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;

use orchy_core::agent::AgentId;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{ReviewId, ReviewRequest, TaskId};

use crate::container::Container;

use super::auth::OrgAuth;

fn parse_org(s: &str) -> Result<OrganizationId, (StatusCode, String)> {
    OrganizationId::new(s).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

fn parse_project(s: &str) -> Result<ProjectId, (StatusCode, String)> {
    ProjectId::try_from(s.to_string()).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

fn parse_task_id(s: &str) -> Result<TaskId, (StatusCode, String)> {
    s.parse::<TaskId>()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid task id: {e}")))
}

fn parse_review_id(s: &str) -> Result<ReviewId, (StatusCode, String)> {
    s.parse::<ReviewId>()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid review id: {e}")))
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), (StatusCode, String)> {
    if auth.0.id() != org_id {
        Err((StatusCode::FORBIDDEN, "forbidden".to_string()))
    } else {
        Ok(())
    }
}

fn map_err(e: orchy_core::error::Error) -> (StatusCode, String) {
    use orchy_core::error::Error;
    match &e {
        Error::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        Error::Conflict(_) => (StatusCode::CONFLICT, e.to_string()),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
pub struct RequestReviewBody {
    pub requester_agent_id: String,
    pub reviewer_agent: Option<String>,
    pub reviewer_role: Option<String>,
}

#[derive(Deserialize)]
pub struct ResolveReviewBody {
    pub resolver_agent_id: String,
    pub approved: bool,
    pub comments: Option<String>,
}

pub async fn list_for_task(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project, task_id_str)): Path<(String, String, String)>,
) -> Result<Json<Vec<ReviewRequest>>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let task_id = parse_task_id(&task_id_str)?;

    let reviews = container
        .task_service
        .list_reviews_for_task(&task_id)
        .await
        .map_err(map_err)?;

    Ok(Json(reviews))
}

pub async fn request(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, task_id_str)): Path<(String, String, String)>,
    Json(body): Json<RequestReviewBody>,
) -> Result<Json<ReviewRequest>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let task_id = parse_task_id(&task_id_str)?;

    let requester = body
        .requester_agent_id
        .parse::<AgentId>()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let reviewer = body
        .reviewer_agent
        .as_deref()
        .map(|s| {
            s.parse::<AgentId>()
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
        })
        .transpose()?;

    let review = container
        .task_service
        .request_review(
            &task_id,
            org_id,
            project_id,
            Namespace::root(),
            requester,
            reviewer,
            body.reviewer_role,
        )
        .await
        .map_err(map_err)?;

    Ok(Json(review))
}

pub async fn get(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project, review_id_str)): Path<(String, String, String)>,
) -> Result<Json<ReviewRequest>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let review_id = parse_review_id(&review_id_str)?;

    let review = container
        .task_service
        .get_review(&review_id)
        .await
        .map_err(map_err)?;

    Ok(Json(review))
}

pub async fn resolve(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project, review_id_str)): Path<(String, String, String)>,
    Json(body): Json<ResolveReviewBody>,
) -> Result<Json<ReviewRequest>, (StatusCode, String)> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let review_id = parse_review_id(&review_id_str)?;

    let resolver = body
        .resolver_agent_id
        .parse::<AgentId>()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let review = container
        .task_service
        .resolve_review(&review_id, resolver, body.approved, body.comments)
        .await
        .map_err(map_err)?;

    Ok(Json(review))
}
