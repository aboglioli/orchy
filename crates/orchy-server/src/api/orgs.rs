use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;

use orchy_application::{
    ApiKeyDto, CreateOrganizationCommand, GenerateApiKeyCommand, GenerateApiKeyResponse,
    GetOrganizationCommand, ListApiKeysCommand, OrganizationDto, RevokeApiKeyCommand,
};

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

#[derive(Deserialize)]
pub struct CreateOrgBody {
    pub id: String,
    pub name: String,
}

pub async fn create(
    State(container): State<Arc<Container>>,
    Json(body): Json<CreateOrgBody>,
) -> Result<Json<OrganizationDto>, ApiError> {
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
) -> Result<Json<Vec<OrganizationDto>>, ApiError> {
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
) -> Result<Json<OrganizationDto>, ApiError> {
    let resp = container
        .app
        .get_organization
        .execute(GetOrganizationCommand { id: org })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}

#[derive(Deserialize)]
pub struct GenerateApiKeyBody {
    pub name: String,
}

pub async fn generate_api_key(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Json(body): Json<GenerateApiKeyBody>,
) -> Result<Json<GenerateApiKeyResponse>, ApiError> {
    let resp = container
        .app
        .generate_api_key
        .execute(GenerateApiKeyCommand {
            org_id: auth.org.id.clone(),
            name: body.name,
            user_id: auth.user_id,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(resp))
}

pub async fn list_api_keys(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
) -> Result<Json<Vec<ApiKeyDto>>, ApiError> {
    let keys = container
        .app
        .list_api_keys
        .execute(ListApiKeysCommand {
            org_id: auth.org.id.clone(),
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(keys))
}

pub async fn revoke_api_key(
    State(container): State<Arc<Container>>,
    _auth: OrgAuth,
    Path(key_id_str): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    container
        .app
        .revoke_api_key
        .execute(RevokeApiKeyCommand { key_id: key_id_str })
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
