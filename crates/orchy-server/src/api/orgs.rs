use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;

use orchy_application::{
    AddApiKeyCommand, CreateOrganizationCommand, GetOrganizationCommand, OrganizationResponse,
    RevokeApiKeyCommand,
};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct CreateOrgBody {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct AddApiKeyBody {
    pub name: String,
    pub key: String,
}

pub async fn create(
    State(container): State<Arc<Container>>,
    Json(body): Json<CreateOrgBody>,
) -> Result<Json<OrganizationResponse>, ApiError> {
    let resp = container
        .app
        .create_organization
        .execute(CreateOrganizationCommand {
            id: body.id,
            name: body.name,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}

pub async fn list(
    State(container): State<Arc<Container>>,
    _auth: OrgAuth,
) -> Result<Json<Vec<OrganizationResponse>>, ApiError> {
    let orgs = container
        .app
        .list_organizations
        .execute(orchy_application::ListOrganizationsCommand {})
        .await
        .map_err(ApiError::from)?;
    Ok(Json(orgs))
}

pub async fn get(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(org): Path<String>,
) -> Result<Json<OrganizationResponse>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }
    let resp = container
        .app
        .get_organization
        .execute(GetOrganizationCommand { id: org })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}

pub async fn add_api_key(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(org): Path<String>,
    Json(body): Json<AddApiKeyBody>,
) -> Result<Json<OrganizationResponse>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }
    let resp = container
        .app
        .add_api_key
        .execute(AddApiKeyCommand {
            org_id: org,
            name: body.name,
            key: body.key,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}

pub async fn revoke_api_key(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, key_id_str)): Path<(String, String)>,
) -> Result<Json<OrganizationResponse>, ApiError> {
    let org_id = OrganizationId::new(&org)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;
    if auth.0.id.as_str() != org_id.as_str() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("access denied to organization {}", org_id),
        ));
    }
    let resp = container
        .app
        .revoke_api_key
        .execute(RevokeApiKeyCommand {
            org_id: org,
            key_id: key_id_str,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}
