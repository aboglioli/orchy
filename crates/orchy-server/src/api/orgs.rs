use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use orchy_core::organization::{ApiKeyId, OrganizationId};

use crate::container::Container;

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

#[derive(Serialize)]
pub struct ApiKeyDto {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(Serialize)]
pub struct OrgDto {
    pub id: String,
    pub name: String,
    pub api_keys: Vec<ApiKeyDto>,
    pub created_at: String,
    pub updated_at: String,
}

fn org_to_dto(org: orchy_core::organization::Organization) -> OrgDto {
    OrgDto {
        id: org.id().to_string(),
        name: org.name().to_string(),
        api_keys: org
            .api_keys()
            .iter()
            .map(|k| ApiKeyDto {
                id: k.id().to_string(),
                name: k.name().to_string(),
                is_active: k.is_active(),
            })
            .collect(),
        created_at: org.created_at().to_rfc3339(),
        updated_at: org.updated_at().to_rfc3339(),
    }
}

pub async fn create(
    State(container): State<Arc<Container>>,
    Json(body): Json<CreateOrgBody>,
) -> Result<Json<OrgDto>, (StatusCode, String)> {
    let id = OrganizationId::new(&body.id).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let org = container
        .org_service
        .create(id, body.name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(org_to_dto(org)))
}

pub async fn list(
    State(container): State<Arc<Container>>,
    _auth: OrgAuth,
) -> Result<Json<Vec<OrgDto>>, (StatusCode, String)> {
    let orgs = container
        .org_service
        .list()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(orgs.into_iter().map(org_to_dto).collect()))
}

pub async fn get(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(org): Path<String>,
) -> Result<Json<OrgDto>, (StatusCode, String)> {
    let org_id = OrganizationId::new(&org).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if auth.0.id() != &org_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".to_string()));
    }
    let org = container
        .org_service
        .get(&org_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "organization not found".to_string()))?;
    Ok(Json(org_to_dto(org)))
}

pub async fn add_api_key(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path(org): Path<String>,
    Json(body): Json<AddApiKeyBody>,
) -> Result<Json<OrgDto>, (StatusCode, String)> {
    let org_id = OrganizationId::new(&org).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if auth.0.id() != &org_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".to_string()));
    }
    let org = container
        .org_service
        .add_api_key(&org_id, body.name, body.key)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(org_to_dto(org)))
}

pub async fn revoke_api_key(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, key_id_str)): Path<(String, String)>,
) -> Result<Json<OrgDto>, (StatusCode, String)> {
    let org_id = OrganizationId::new(&org).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if auth.0.id() != &org_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".to_string()));
    }
    let uuid = key_id_str
        .parse::<Uuid>()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let key_id = ApiKeyId::from_uuid(uuid);
    let org = container
        .org_service
        .revoke_api_key(&org_id, &key_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(org_to_dto(org)))
}
