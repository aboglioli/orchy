use orchy_application::{
    AddEdgeCommand, GetGraphCommand, GetNeighborsCommand, ListEdgesCommand, RemoveEdgeCommand,
};

use crate::mcp::handler::{OrchyHandler, mcp_error, to_json};
use crate::mcp::params::{
    AddEdgeParams, GetGraphParams, GetNeighborsParams, ListEdgesParams, RemoveEdgeParams,
};

use super::parse_as_of;

pub(super) async fn add_edge(
    h: &OrchyHandler,
    params: AddEdgeParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let cmd = AddEdgeCommand {
        org_id: org.to_string(),
        from_kind: params.from_kind,
        from_id: params.from_id,
        to_kind: params.to_kind,
        to_id: params.to_id,
        rel_type: params.rel_type,
        display: params.display,
        created_by: h.get_session_agent().await.map(|id| id.to_string()),
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

pub(super) async fn get_neighbors(
    h: &OrchyHandler,
    params: GetNeighborsParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let include_nodes = params.include_nodes.unwrap_or(false);
    let as_of = parse_as_of(params.as_of)
        .map_err(|e| mcp_error(orchy_core::error::Error::InvalidInput(e)))?;
    let cmd = GetNeighborsCommand {
        org_id: org.to_string(),
        kind: params.kind,
        id: params.id,
        direction: params.direction,
        rel_type: params.rel_type,
        include_nodes,
        node_content_limit: params.node_content_limit.map(|n| n as usize),
        only_active: params.only_active.unwrap_or(true),
        as_of,
    };

    match h.container.app.get_neighbors.execute(cmd).await {
        Ok(resp) => {
            if include_nodes {
                Ok(to_json(&resp))
            } else {
                Ok(to_json(&resp.edges))
            }
        }
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn get_graph(
    h: &OrchyHandler,
    params: GetGraphParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let as_of = parse_as_of(params.as_of)
        .map_err(|e| mcp_error(orchy_core::error::Error::InvalidInput(e)))?;
    let cmd = GetGraphCommand {
        org_id: org.to_string(),
        kind: params.kind,
        id: params.id,
        max_depth: params.max_depth,
        rel_types: params.rel_types,
        direction: params.direction,
        include_nodes: params.include_nodes.unwrap_or(false),
        node_content_limit: params.node_content_limit.map(|n| n as usize),
        only_active: params.only_active.unwrap_or(true),
        max_results: params.max_results,
        as_of,
    };

    match h.container.app.get_graph.execute(cmd).await {
        Ok(graph) => Ok(to_json(&graph)),
        Err(e) => Err(mcp_error(e)),
    }
}

pub(super) async fn list_edges(
    h: &OrchyHandler,
    params: ListEdgesParams,
) -> Result<String, String> {
    let (_, org, _, _) = h.require_session().await?;

    let as_of = parse_as_of(params.as_of)
        .map_err(|e| mcp_error(orchy_core::error::Error::InvalidInput(e)))?;
    let cmd = ListEdgesCommand {
        org_id: org.to_string(),
        rel_type: params.rel_type,
        after: params.after,
        limit: params.limit,
        only_active: params.only_active.unwrap_or(false),
        as_of,
    };

    match h.container.app.list_edges.execute(cmd).await {
        Ok(page) => Ok(to_json(&page)),
        Err(e) => Err(mcp_error(e)),
    }
}
