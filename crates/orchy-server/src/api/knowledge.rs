use std::collections::HashMap;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_core::agent::AgentId;
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeKind, Version as KnowledgeVersion, WriteKnowledge,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_project(s: &str) -> Result<ProjectId, ApiError> {
    ProjectId::try_from(s.to_string())
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn parse_ns(ns: Option<&str>) -> Result<Namespace, ApiError> {
    match ns {
        Some(s) => Namespace::try_from(format!("/{s}"))
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string())),
        None => Ok(Namespace::root()),
    }
}

fn parse_optional_ns(ns: Option<&str>) -> Result<Option<Namespace>, ApiError> {
    match ns {
        Some(s) => Namespace::try_from(format!("/{s}"))
            .map(Some)
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string())),
        None => Ok(None),
    }
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), ApiError> {
    if auth.0.id() != org_id {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "forbidden".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn map_err(e: orchy_core::error::Error) -> ApiError {
    use orchy_core::error::Error;
    match &e {
        Error::NotFound(_) => ApiError(StatusCode::NOT_FOUND, "NOT_FOUND", e.to_string()),
        Error::VersionMismatch { .. } => ApiError(StatusCode::CONFLICT, "CONFLICT", e.to_string()),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            e.to_string(),
        ),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub namespace: Option<String>,
    pub path_prefix: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Deserialize)]
pub struct NamespaceQuery {
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct WriteBody {
    pub ns: Option<String>,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub version: Option<u64>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct SearchBody {
    pub query: String,
    pub ns: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct ImportBody {
    pub source_project: String,
    pub path: String,
    pub source_ns: Option<String>,
    pub ns: Option<String>,
}

#[derive(Deserialize)]
pub struct AppendBody {
    pub value: String,
    pub kind: String,
    pub ns: Option<String>,
    pub separator: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct MoveBody {
    pub new_namespace: String,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct RenameBody {
    pub new_path: String,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct ChangeKindBody {
    pub kind: String,
    pub version: Option<u64>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct PatchMetadataBody {
    pub set: Option<HashMap<String, String>>,
    pub remove: Option<Vec<String>>,
    pub version: Option<u64>,
}

#[derive(Serialize)]
pub struct KnowledgeTypeDto {
    pub r#type: String,
    pub description: String,
}

pub async fn list(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Knowledge>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_optional_ns(query.namespace.as_deref())?;

    let kind = match query.kind.as_deref() {
        Some(k) => Some(
            k.parse::<KnowledgeKind>()
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?,
        ),
        None => None,
    };

    let agent_id = query
        .agent_id
        .as_deref()
        .map(|s| {
            s.parse::<AgentId>()
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
        })
        .transpose()?;

    let filter = KnowledgeFilter {
        org_id: Some(org_id),
        project: Some(project_id),
        namespace: ns,
        kind,
        tag: query.tag,
        path_prefix: query.path_prefix,
        agent_id,
        ..Default::default()
    };

    let entries = container
        .knowledge_service
        .list(filter)
        .await
        .map_err(map_err)?;

    Ok(Json(entries))
}

pub async fn list_types(
    auth: OrgAuth,
    Path((org, _project)): Path<(String, String)>,
) -> Result<Json<Vec<KnowledgeTypeDto>>, ApiError> {
    let org_id = parse_org(&org)?;
    if auth.0.id() != &org_id {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "forbidden".to_string(),
        ));
    }

    let types = KnowledgeKind::all()
        .iter()
        .map(|k| KnowledgeTypeDto {
            r#type: k.to_string(),
            description: k.description().to_string(),
        })
        .collect();

    Ok(Json(types))
}

pub async fn search(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, _project)): Path<(String, String)>,
    Json(body): Json<SearchBody>,
) -> Result<Json<Vec<Knowledge>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let ns = parse_optional_ns(body.ns.as_deref())?;
    let limit = body.limit.unwrap_or(10) as usize;

    let mut entries = container
        .knowledge_service
        .search(&org_id, &body.query, ns.as_ref(), limit)
        .await
        .map_err(map_err)?;

    if let Some(k) = body.kind.as_deref() {
        let kind: KnowledgeKind = k
            .parse()
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?;
        entries.retain(|e| e.kind() == kind);
    }

    Ok(Json(entries))
}

pub async fn import(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<ImportBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let dest_ns = parse_ns(body.ns.as_deref())?;
    let source_project = parse_project(&body.source_project)?;
    let source_ns = parse_ns(body.source_ns.as_deref())?;

    let source_entry = container
        .knowledge_service
        .read(&org_id, Some(&source_project), &source_ns, &body.path)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("entry not found: {}", body.path),
            )
        })?;

    let cmd = WriteKnowledge {
        org_id,
        project: Some(project_id),
        namespace: dest_ns,
        path: source_entry.path().to_string(),
        kind: source_entry.kind(),
        title: source_entry.title().to_string(),
        content: source_entry.content().to_string(),
        tags: source_entry.tags().to_vec(),
        expected_version: None,
        agent_id: None,
        metadata: source_entry.metadata().clone(),
        metadata_remove: vec![],
    };

    let entry = container
        .knowledge_service
        .write(cmd)
        .await
        .map_err(map_err)?;

    Ok(Json(entry))
}

pub async fn read(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<Option<Knowledge>>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let entry = container
        .knowledge_service
        .read(&org_id, Some(&project_id), &ns, &path)
        .await
        .map_err(map_err)?;

    Ok(Json(entry))
}

pub async fn write(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<WriteBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(body.ns.as_deref())?;

    let kind: KnowledgeKind = body
        .kind
        .parse()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?;

    let cmd = WriteKnowledge {
        org_id,
        project: Some(project_id),
        namespace: ns,
        path,
        kind,
        title: body.title,
        content: body.content,
        tags: body.tags.unwrap_or_default(),
        expected_version: body.version.map(KnowledgeVersion::from),
        agent_id: None,
        metadata: body.metadata.unwrap_or_default(),
        metadata_remove: vec![],
    };

    let entry = container
        .knowledge_service
        .write(cmd)
        .await
        .map_err(map_err)?;

    Ok(Json(entry))
}

pub async fn delete(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(query.namespace.as_deref())?;

    let entry = container
        .knowledge_service
        .read(&org_id, Some(&project_id), &ns, &path)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("entry not found: {path}"),
            )
        })?;

    container
        .knowledge_service
        .delete(&entry.id())
        .await
        .map_err(map_err)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn append(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<AppendBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = parse_ns(body.ns.as_deref())?;

    let kind: KnowledgeKind = body
        .kind
        .parse()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?;

    let separator = body.separator.as_deref().unwrap_or("\n");

    let entry = container
        .knowledge_service
        .append(
            &org_id,
            Some(&project_id),
            &ns,
            &path,
            kind,
            body.value,
            separator,
            None,
            body.metadata,
            None,
        )
        .await
        .map_err(map_err)?;

    Ok(Json(entry))
}

pub async fn move_entry(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = Namespace::root();

    let entry = container
        .knowledge_service
        .read(&org_id, Some(&project_id), &ns, &path)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("entry not found: {path}"),
            )
        })?;

    let new_ns = Namespace::try_from(format!("/{}", body.new_namespace))
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))?;

    let updated = container
        .knowledge_service
        .move_entry(&entry.id(), new_ns, body.metadata, None)
        .await
        .map_err(map_err)?;

    Ok(Json(updated))
}

pub async fn rename(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<RenameBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = Namespace::root();

    let entry = container
        .knowledge_service
        .read(&org_id, Some(&project_id), &ns, &path)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("entry not found: {path}"),
            )
        })?;

    let updated = container
        .knowledge_service
        .rename(&entry.id(), body.new_path, body.metadata, None)
        .await
        .map_err(map_err)?;

    Ok(Json(updated))
}

pub async fn change_kind(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<ChangeKindBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = Namespace::root();

    let kind: KnowledgeKind = body
        .kind
        .parse()
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e))?;

    let updated = container
        .knowledge_service
        .change_kind(
            &org_id,
            Some(&project_id),
            &ns,
            &path,
            kind,
            body.version.map(KnowledgeVersion::from),
            body.metadata,
            None,
        )
        .await
        .map_err(map_err)?;

    Ok(Json(updated))
}

pub async fn patch_metadata(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<PatchMetadataBody>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = Namespace::root();

    let updated = container
        .knowledge_service
        .patch_metadata(
            &org_id,
            Some(&project_id),
            &ns,
            &path,
            body.set.unwrap_or_default(),
            body.remove.unwrap_or_default(),
            body.version.map(KnowledgeVersion::from),
        )
        .await
        .map_err(map_err)?;

    Ok(Json(updated))
}

pub async fn tag(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path, tag_name)): Path<(String, String, String, String)>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = Namespace::root();

    let entry = container
        .knowledge_service
        .read(&org_id, Some(&project_id), &ns, &path)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("entry not found: {path}"),
            )
        })?;

    let updated = container
        .knowledge_service
        .tag(&entry.id(), tag_name, None, None)
        .await
        .map_err(map_err)?;

    Ok(Json(updated))
}

pub async fn untag(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path, tag_name)): Path<(String, String, String, String)>,
) -> Result<Json<Knowledge>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;
    let project_id = parse_project(&project)?;
    let ns = Namespace::root();

    let entry = container
        .knowledge_service
        .read(&org_id, Some(&project_id), &ns, &path)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                format!("entry not found: {path}"),
            )
        })?;

    let updated = container
        .knowledge_service
        .untag(&entry.id(), &tag_name, None, None)
        .await
        .map_err(map_err)?;

    Ok(Json(updated))
}
