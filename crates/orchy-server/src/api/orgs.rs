use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;

use orchy_application::{
    AddApiKeyCommand, CreateOrganizationCommand, GetOrganizationCommand, OrganizationResponse,
    RevokeApiKeyCommand,
};

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
    _auth: OrgAuth,
    Path(org): Path<String>,
) -> Result<Json<OrganizationResponse>, ApiError> {
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
    Json(body): Json<AddApiKeyBody>,
) -> Result<Json<OrganizationResponse>, ApiError> {
    let resp = container
        .app
        .add_api_key
        .execute(AddApiKeyCommand {
            org_id: auth.org.id.clone(),
            name: body.name,
            key: body.key,
            user_id: auth.user_id.clone(),
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}

pub async fn revoke_api_key(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(key_id_str): Path<String>,
) -> Result<Json<OrganizationResponse>, ApiError> {
    let resp = container
        .app
        .revoke_api_key
        .execute(RevokeApiKeyCommand {
            org_id: auth.org.id.clone(),
            key_id: key_id_str,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}
