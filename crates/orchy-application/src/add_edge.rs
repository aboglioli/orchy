use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::graph::check_no_cycle;
use orchy_core::graph::{Edge, EdgeStore, RelationType, TraversalDirection};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::TaskId;

use crate::dto::EdgeDto;

pub struct AddEdgeCommand {
    pub org_id: String,
    pub from_kind: String,
    pub from_id: String,
    pub to_kind: String,
    pub to_id: String,
    pub rel_type: String,
    pub created_by: Option<String>,
    pub if_not_exists: bool,
}

pub struct AddEdge {
    store: Arc<dyn EdgeStore>,
}

impl AddEdge {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: AddEdgeCommand) -> Result<EdgeDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let from_kind = cmd
            .from_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let to_kind = cmd
            .to_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let rel_type = parse_rel_type_with_aliases(&cmd.rel_type)?;
        let created_by = cmd.created_by.map(|s| AgentId::from_str(&s)).transpose()?;

        if let Some(existing) = self
            .store
            .find_by_pair(
                &org_id,
                &from_kind,
                &cmd.from_id,
                &to_kind,
                &cmd.to_id,
                &rel_type,
            )
            .await?
        {
            if cmd.if_not_exists {
                return Ok(EdgeDto::from(&existing));
            }
            return Err(Error::Conflict(format!(
                "edge {from_kind}:{} --{rel_type}--> {to_kind}:{} already exists",
                cmd.from_id, cmd.to_id
            )));
        }

        if rel_type == RelationType::DependsOn
            && from_kind == ResourceKind::Task
            && to_kind == ResourceKind::Task
        {
            let from_task_id: TaskId = cmd
                .from_id
                .parse()
                .map_err(|_| Error::InvalidInput("invalid task id in from_id".to_string()))?;
            let reachable = self
                .store
                .find_neighbors(
                    &org_id,
                    &ResourceKind::Task,
                    &cmd.to_id,
                    &[RelationType::DependsOn],
                    &[ResourceKind::Task],
                    TraversalDirection::Outgoing,
                    10,
                    None,
                    200,
                )
                .await?;
            let reachable_ids: Vec<TaskId> = reachable
                .iter()
                .filter_map(|hop| {
                    let peer_id = match hop.direction {
                        orchy_core::graph::RelationDirection::Outgoing => hop.edge.to_id(),
                        orchy_core::graph::RelationDirection::Incoming => hop.edge.from_id(),
                    };
                    peer_id.parse::<TaskId>().ok()
                })
                .collect();
            check_no_cycle(&from_task_id, &reachable_ids)?;
        }

        let mut edge = Edge::new(
            org_id,
            from_kind,
            cmd.from_id,
            to_kind,
            cmd.to_id,
            rel_type,
            created_by,
        )?;
        self.store.save(&mut edge).await?;
        Ok(EdgeDto::from(&edge))
    }
}

fn parse_rel_type_with_aliases(s: &str) -> Result<RelationType> {
    let canonical = match s {
        "blocks" | "requires" | "needs" => "depends_on",
        "creates" | "made" | "wrote" => "produces",
        "fulfills" | "executes" => "implements",
        "child_of" | "parent_of" => "spawns",
        "based_on" | "from" => "derived_from",
        other => other,
    };
    canonical
        .parse::<RelationType>()
        .map_err(Error::InvalidInput)
}
