use orchy_application::{AddEdgeCommand, MaterializeNeighborhoodCommand, RemoveEdgeCommand};
use orchy_core::graph::RelationOptions;

use crate::mcp::handler::{OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{AddEdgeParams, QueryRelationsParams, RemoveEdgeParams};

use super::{parse_as_of, parse_direction, parse_rel_type_alias};

pub(super) async fn add_edge(h: &OrchyHandler, params: AddEdgeParams) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let cmd = AddEdgeCommand {
        org_id: org.to_string(),
        from_kind: params.from_kind,
        from_id: params.from_id,
        to_kind: params.to_kind,
        to_id: params.to_id,
        rel_type: params.rel_type,
        created_by: h.get_session_agent().await.map(|id| id.to_string()),
        if_not_exists: params.if_not_exists.unwrap_or(true),
    };

    match h.container.app.add_edge.execute(cmd).await {
        Ok(edge) => Ok(to_json(&edge)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn remove_edge(
    h: &OrchyHandler,
    params: RemoveEdgeParams,
) -> Result<String, String> {
    h.require_session().await?;

    let cmd = RemoveEdgeCommand {
        edge_id: params.edge_id,
    };

    match h.container.app.remove_edge.execute(cmd).await {
        Ok(()) => Ok(r#"{"deleted":true}"#.to_string()),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn query_relations(
    h: &OrchyHandler,
    params: QueryRelationsParams,
) -> Result<String, String> {
    let (_, org, session_project, session_namespace) = h.require_session().await?;

    let as_of = parse_as_of(params.as_of)
        .map_err(|e| mcp_error(orchy_core::error::Error::InvalidInput(e)))?;

    let options = RelationOptions {
        rel_types: params.rel_types.map(|v| {
            v.into_iter()
                .filter_map(|s| parse_rel_type_alias(&s).ok())
                .collect()
        }),
        target_kinds: params
            .target_kinds
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| s.parse::<orchy_core::resource_ref::ResourceKind>().ok())
            .collect(),
        direction: parse_direction(params.direction.as_deref()),
        max_depth: params.max_depth.unwrap_or(1),
        limit: params.limit.unwrap_or(50),
    };

    let cmd = MaterializeNeighborhoodCommand {
        org_id: org.to_string(),
        anchor_kind: params.anchor_kind,
        anchor_id: params.anchor_id,
        options,
        as_of,
        project: Some(
            params
                .project
                .unwrap_or_else(|| session_project.to_string()),
        ),
        namespace: Some(
            params
                .namespace
                .unwrap_or_else(|| session_namespace.to_string()),
        ),
        semantic_query: params.semantic_query,
    };

    match h.container.app.materialize_neighborhood.execute(cmd).await {
        Ok(neighborhood) => Ok(to_json(&neighborhood)),
        Err(e) => Err(mcp_error(e)),
    }
}
