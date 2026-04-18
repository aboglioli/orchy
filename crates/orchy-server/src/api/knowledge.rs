use std::collections::HashMap;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};

use orchy_application::{
    AppendKnowledgeCommand, ChangeKnowledgeKindCommand, DeleteKnowledgeCommand,
    ImportKnowledgeCommand, ListKnowledgeCommand, MoveKnowledgeCommand,
    PatchKnowledgeMetadataCommand, ReadKnowledgeCommand, RenameKnowledgeCommand,
    SearchKnowledgeCommand, TagKnowledgeCommand, UntagKnowledgeCommand, WriteKnowledgeCommand,
};
use orchy_core::knowledge::KnowledgeKind;
use orchy_core::organization::OrganizationId;

use crate::container::Container;

use super::ApiError;
use super::auth::OrgAuth;

fn parse_org(s: &str) -> Result<OrganizationId, ApiError> {
    OrganizationId::new(s)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_PARAM", e.to_string()))
}

fn check_org(auth: &OrgAuth, org_id: &OrganizationId) -> Result<(), ApiError> {
    if auth.0.id.as_str() != org_id.as_str() {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "forbidden".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub namespace: Option<String>,
    pub path_prefix: Option<String>,
    pub author_agent_id: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct NamespaceQuery {
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct WriteBody {
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
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
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct ImportBody {
    pub source_project: String,
    pub path: String,
    #[serde(alias = "source_ns")]
    pub source_namespace: Option<String>,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
}

#[derive(Deserialize)]
pub struct AppendBody {
    pub value: String,
    pub kind: String,
    #[serde(alias = "ns")]
    pub namespace: Option<String>,
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
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ListKnowledgeCommand {
        org_id: org,
        project: Some(project),
        include_org_level: false,
        namespace: query.namespace,
        kind: query.kind,
        tag: query.tag,
        path_prefix: query.path_prefix,
        agent_id: query.author_agent_id,
        after: query.after,
        limit: query.limit,
    };

    let page = container
        .app
        .list_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&page).unwrap_or_default()))
}

pub async fn list_types(
    auth: OrgAuth,
    Path((org, _project)): Path<(String, String)>,
) -> Result<Json<Vec<KnowledgeTypeDto>>, ApiError> {
    let org_id = parse_org(&org)?;
    if auth.0.id.as_str() != org_id.as_str() {
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
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<SearchBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = SearchKnowledgeCommand {
        org_id: org,
        query: body.query,
        namespace: body.namespace,
        kind: body.kind,
        limit: body.limit,
        project: Some(project),
    };

    let entries = container
        .app
        .search_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entries).unwrap_or_default()))
}

pub async fn import(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project)): Path<(String, String)>,
    Json(body): Json<ImportBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ImportKnowledgeCommand {
        source_org_id: org.clone(),
        source_project: body.source_project,
        source_namespace: body.source_namespace,
        source_path: body.path,
        target_org_id: org,
        target_project: project,
        target_namespace: body.namespace,
        target_path: None,
        agent_id: None,
    };

    let entry = container
        .app
        .import_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn read(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ReadKnowledgeCommand {
        org_id: org,
        project,
        namespace: query.namespace,
        path,
    };

    let entry = container
        .app
        .read_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn write(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<WriteBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = WriteKnowledgeCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        path,
        kind: body.kind,
        title: body.title,
        content: body.content,
        tags: body.tags,
        version: body.version,
        agent_id: None,
        metadata: body.metadata,
        metadata_remove: None,
    };

    let entry = container
        .app
        .write_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn delete(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Query(query): Query<NamespaceQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = DeleteKnowledgeCommand {
        org_id: org,
        project,
        namespace: query.namespace,
        path,
    };

    container
        .app
        .delete_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn append(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<AppendBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = AppendKnowledgeCommand {
        org_id: org,
        project,
        namespace: body.namespace,
        path,
        kind: body.kind,
        value: body.value,
        separator: body.separator,
        agent_id: None,
        metadata: body.metadata,
        metadata_remove: None,
    };

    let entry = container
        .app
        .append_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn move_entry(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<MoveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = MoveKnowledgeCommand {
        org_id: org,
        project,
        namespace: None,
        path,
        new_namespace: body.new_namespace,
    };

    let entry = container
        .app
        .move_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn rename(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<RenameBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = RenameKnowledgeCommand {
        org_id: org,
        project,
        namespace: None,
        path,
        new_path: body.new_path,
    };

    let entry = container
        .app
        .rename_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn change_kind(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<ChangeKindBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = ChangeKnowledgeKindCommand {
        org_id: org,
        project,
        namespace: None,
        path,
        new_kind: body.kind,
        version: body.version,
    };

    let entry = container
        .app
        .change_knowledge_kind
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn patch_metadata(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path)): Path<(String, String, String)>,
    Json(body): Json<PatchMetadataBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = PatchKnowledgeMetadataCommand {
        org_id: org,
        project,
        namespace: None,
        path,
        set: body.set.unwrap_or_default(),
        remove: body.remove.unwrap_or_default(),
        version: body.version,
    };

    let entry = container
        .app
        .patch_knowledge_metadata
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn tag(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path, tag_name)): Path<(String, String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = TagKnowledgeCommand {
        org_id: org,
        project,
        namespace: None,
        path,
        tag: tag_name,
    };

    let entry = container
        .app
        .tag_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}

pub async fn untag(
    State(container): State<Arc<Container>>,
    auth: OrgAuth,
    Path((org, project, path, tag_name)): Path<(String, String, String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let org_id = parse_org(&org)?;
    check_org(&auth, &org_id)?;

    let cmd = UntagKnowledgeCommand {
        org_id: org,
        project,
        namespace: None,
        path,
        tag: tag_name,
    };

    let entry = container
        .app
        .untag_knowledge
        .execute(cmd)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(serde_json::to_value(&entry).unwrap_or_default()))
}
