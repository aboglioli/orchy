use orchy_application::{
    AppendKnowledgeCommand, ArchiveKnowledgeCommand, AssembleContextCommand,
    ChangeKnowledgeKindCommand, ConsolidateKnowledgeCommand, DeleteKnowledgeCommand,
    ImportKnowledgeCommand, ListKnowledgeCommand, MoveKnowledgeCommand,
    PatchKnowledgeMetadataCommand, PromoteKnowledgeCommand, ReadKnowledgeCommand,
    RenameKnowledgeCommand, SearchKnowledgeCommand, TagKnowledgeCommand, UnarchiveKnowledgeCommand,
    UntagKnowledgeCommand, WriteKnowledgeCommand,
};
use orchy_core::knowledge::KnowledgeKind;

use crate::mcp::handler::{NamespacePolicy, OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{
    AppendKnowledgeParams, ArchiveKnowledgeParams, AssembleContextParams,
    ChangeKnowledgeKindParams, ConsolidateKnowledgeParams, DeleteKnowledgeParams,
    ImportKnowledgeParams, ListKnowledgeParams, ListKnowledgeTypesParams, MoveKnowledgeParams,
    PatchKnowledgeMetadataParams, PromoteKnowledgeParams, ReadKnowledgeParams,
    RenameKnowledgeParams, SearchKnowledgeParams, TagKnowledgeParams, UnarchiveKnowledgeParams,
    UntagKnowledgeParams, WriteKnowledgeParams,
};

use super::{
    knowledge_metadata_from_json_str, optional_knowledge_metadata, parse_relation_options,
};

pub(super) async fn list_knowledge_types(
    _h: &OrchyHandler,
    _params: ListKnowledgeTypesParams,
) -> Result<String, String> {
    let types: Vec<serde_json::Value> = KnowledgeKind::all()
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": t.to_string(),
                "description": t.description(),
            })
        })
        .collect();
    Ok(to_json(&types))
}

pub(super) async fn write_knowledge(
    h: &OrchyHandler,
    params: WriteKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
        .await?;

    let metadata = knowledge_metadata_from_json_str(params.metadata.as_deref(), "metadata")?;

    let cmd = WriteKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        kind: params.kind,
        title: params.title,
        content: params.content,
        tags: params.tags,
        version: params.version,
        agent_id: h.get_session_agent().await.map(|id| id.to_string()),
        metadata: if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        },
        metadata_remove: params.metadata_remove,
        task_id: params.task_id,
        valid_from: params.valid_from,
        valid_until: params.valid_until,
    };

    match h.container.app.write_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn patch_knowledge_metadata(
    h: &OrchyHandler,
    params: PatchKnowledgeMetadataParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let set = optional_knowledge_metadata(params.metadata, "metadata")?.unwrap_or_default();
    let remove = params.metadata_remove.unwrap_or_default();

    let cmd = PatchKnowledgeMetadataCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        set,
        remove,
        version: params.version,
        valid_from: None,
        valid_until: None,
    };

    match h.container.app.patch_knowledge_metadata.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn read_knowledge(
    h: &OrchyHandler,
    params: ReadKnowledgeParams,
) -> Result<String, String> {
    let (_, session_project, _) = h.require_session().await?;
    let org = h.org();
    let project = params
        .project
        .unwrap_or_else(|| session_project.to_string());

    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let relations = parse_relation_options(params.relations);
    let cmd = ReadKnowledgeCommand {
        org_id: org.to_string(),
        project,
        namespace: Some(namespace.to_string()),
        path: params.path,
        relations,
    };

    match h.container.app.read_knowledge.execute(cmd).await {
        Ok(resp) => Ok(to_json(&resp)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn list_knowledge(
    h: &OrchyHandler,
    params: ListKnowledgeParams,
) -> Result<String, String> {
    let (_, _, _) = h.require_session().await?;
    let org = h.org();
    let namespace = match params.namespace.as_deref() {
        Some(_) => Some(
            h.resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
                .await?
                .to_string(),
        ),
        None => None,
    };

    let project = if params.project.is_some() {
        params.project
    } else if namespace.is_none() {
        h.get_session_project().await.map(|p| p.to_string())
    } else {
        None
    };

    let cmd = ListKnowledgeCommand {
        org_id: org.to_string(),
        project,
        include_org_level: false,
        namespace,
        kind: params.kind,
        tag: params.tag,
        path_prefix: params.path_prefix,
        after: params.after,
        limit: params.limit,
        orphaned: params.orphaned,
        archived: params.archived,
    };

    match h.container.app.list_knowledge.execute(cmd).await {
        Ok(page) => Ok(to_json(&page)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn search_knowledge(
    h: &OrchyHandler,
    params: SearchKnowledgeParams,
) -> Result<String, String> {
    let (_, _, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::SessionDefault)
        .await?;

    let cmd = SearchKnowledgeCommand {
        org_id: org.to_string(),
        query: params.query,
        namespace: Some(namespace.to_string()),
        kind: params.kind,
        limit: params.limit,
        project: params.project,
        min_score: params.min_score,
        anchor_kind: params.anchor_kind,
        anchor_id: params.anchor_id,
        task_id: params.task_id,
    };

    match h.container.app.search_knowledge.execute(cmd).await {
        Ok(entries) => Ok(to_json(&entries)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn delete_knowledge(
    h: &OrchyHandler,
    params: DeleteKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = DeleteKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
    };

    match h.container.app.delete_knowledge.execute(cmd).await {
        Ok(()) => Ok(r#"{"ok":true}"#.to_string()),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn archive_knowledge(
    h: &OrchyHandler,
    params: ArchiveKnowledgeParams,
) -> Result<String, String> {
    let (_agent_id, project, namespace) = h.require_session().await?;
    let org = h.org();
    let cmd = ArchiveKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        reason: params.reason,
    };
    match h.container.app.archive_knowledge.execute(cmd).await {
        Ok(response) => Ok(to_json(&response)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn unarchive_knowledge(
    h: &OrchyHandler,
    params: UnarchiveKnowledgeParams,
) -> Result<String, String> {
    let (_agent_id, project, namespace) = h.require_session().await?;
    let org = h.org();
    let cmd = UnarchiveKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
    };
    match h.container.app.unarchive_knowledge.execute(cmd).await {
        Ok(response) => Ok(to_json(&response)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn append_knowledge(
    h: &OrchyHandler,
    params: AppendKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::RegisterIfNew)
        .await?;

    let metadata = optional_knowledge_metadata(params.metadata, "metadata")?;

    let cmd = AppendKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        kind: params.kind,
        value: params.value,
        separator: params.separator,
        metadata,
        metadata_remove: params.metadata_remove,
    };

    match h.container.app.append_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn move_knowledge(
    h: &OrchyHandler,
    params: MoveKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let new_namespace = h
        .resolve_namespace(Some(&params.new_namespace), NamespacePolicy::RegisterIfNew)
        .await?;

    let cmd = MoveKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        new_namespace: new_namespace.to_string(),
    };

    match h.container.app.move_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn rename_knowledge(
    h: &OrchyHandler,
    params: RenameKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = RenameKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        new_path: params.new_path,
    };

    match h.container.app.rename_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn change_knowledge_kind(
    h: &OrchyHandler,
    params: ChangeKnowledgeKindParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = ChangeKnowledgeKindCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        new_kind: params.kind,
        version: params.version,
    };

    match h.container.app.change_knowledge_kind.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn tag_knowledge(
    h: &OrchyHandler,
    params: TagKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = TagKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        tag: params.tag,
    };

    match h.container.app.tag_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn untag_knowledge(
    h: &OrchyHandler,
    params: UntagKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = UntagKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        path: params.path,
        tag: params.tag,
    };

    match h.container.app.untag_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn import_knowledge(
    h: &OrchyHandler,
    params: ImportKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(None, NamespacePolicy::RegisterIfNew)
        .await?;

    let cmd = ImportKnowledgeCommand {
        source_org_id: org.to_string(),
        source_project: params.source_project,
        source_namespace: params.source_namespace,
        source_path: params.path,
        target_org_id: org.to_string(),
        target_project: project.to_string(),
        target_namespace: Some(namespace.to_string()),
        target_path: None,
    };

    match h.container.app.import_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn assemble_context(
    h: &OrchyHandler,
    params: AssembleContextParams,
) -> Result<String, String> {
    let (_, _, _) = h.require_session().await?;
    let org = h.org();
    let cmd = AssembleContextCommand {
        org_id: org.to_string(),
        kind: params.kind,
        id: params.id,
        max_tokens: params.max_tokens.map(|n| n as usize),
    };

    match h.container.app.assemble_context.execute(cmd).await {
        Ok(resp) => Ok(to_json(&resp)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn promote_knowledge(
    h: &OrchyHandler,
    params: PromoteKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = PromoteKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        source_path: params.source_path,
        target_path: params.target_path,
        target_title: params.target_title,
        instruction: params.instruction,
    };

    match h.container.app.promote_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn consolidate_knowledge(
    h: &OrchyHandler,
    params: ConsolidateKnowledgeParams,
) -> Result<String, String> {
    let (_, project, _) = h.require_session().await?;
    let org = h.org();
    let namespace = h
        .resolve_namespace(params.namespace.as_deref(), NamespacePolicy::Required)
        .await?;

    let cmd = ConsolidateKnowledgeCommand {
        org_id: org.to_string(),
        project: project.to_string(),
        namespace: Some(namespace.to_string()),
        source_paths: params.source_paths,
        target_path: params.target_path,
        target_title: params.target_title,
        target_kind: params.target_kind,
    };

    match h.container.app.consolidate_knowledge.execute(cmd).await {
        Ok(entry) => Ok(to_json(&entry)),
        Err(e) => Err(mcp_error(e)),
    }
}
