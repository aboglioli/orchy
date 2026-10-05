use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{
    Clock, Edge, EdgeAdded, EdgeRemoved, EdgeStore, EntityRef, EventLog, Relation, Result,
    TraversalHop,
};

pub struct MemoryEdgeStore {
    edges: Mutex<Vec<Edge>>,
    log: Arc<dyn EventLog>,
    clock: Arc<dyn Clock>,
}

impl MemoryEdgeStore {
    pub(crate) fn state(&self) -> Vec<Edge> {
        self.edges.lock().expect("edges lock").clone()
    }

    pub(crate) fn restore_state(&self, state: Vec<Edge>) {
        *self.edges.lock().expect("edges lock") = state;
    }

    pub fn new(log: Arc<dyn EventLog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            edges: Mutex::new(Vec::new()),
            log,
            clock,
        }
    }

    fn all(&self) -> Vec<Edge> {
        self.edges.lock().expect("edge mutex").clone()
    }
}

#[async_trait]
impl EdgeStore for MemoryEdgeStore {
    async fn add(&self, edge: &Edge) -> Result<()> {
        let added = {
            let mut edges = self.edges.lock().expect("edge mutex");
            let added = !edges.contains(edge);
            if added {
                edges.push(edge.clone());
            }
            added
        };
        if !added {
            return Ok(());
        }
        self.log
            .append(&[Box::new(EdgeAdded::of(edge, self.clock.now()))])
            .await
    }

    async fn remove(&self, edge: &Edge) -> Result<()> {
        let removed = {
            let mut edges = self.edges.lock().expect("edge mutex");
            let before = edges.len();
            edges.retain(|e| e != edge);
            edges.len() != before
        };
        if !removed {
            return Ok(());
        }
        self.log
            .append(&[Box::new(EdgeRemoved::of(edge, self.clock.now()))])
            .await
    }

    async fn of_relation(&self, relation: &Relation) -> Result<Vec<Edge>> {
        Ok(self
            .edges
            .lock()
            .expect("edges lock")
            .iter()
            .filter(|e| e.relation() == relation)
            .cloned()
            .collect())
    }

    async fn out(&self, from: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>> {
        Ok(self
            .all()
            .into_iter()
            .filter(|e| e.from() == from)
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn incoming(&self, to: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>> {
        Ok(self
            .all()
            .into_iter()
            .filter(|e| e.to() == to)
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn neighbourhood(&self, of: &EntityRef, depth: u8) -> Result<Vec<TraversalHop>> {
        let edges = self.all();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut frontier = vec![of.clone()];
        let mut hops = Vec::new();
        seen.insert(of.to_string());

        for level in 1..=depth {
            let mut next = Vec::new();
            for node in &frontier {
                for edge in edges.iter().filter(|e| e.from() == node || e.to() == node) {
                    let other = if edge.from() == node {
                        edge.to()
                    } else {
                        edge.from()
                    };
                    hops.push(TraversalHop {
                        edge: edge.clone(),
                        depth: level,
                    });
                    if seen.insert(other.to_string()) {
                        next.push(other.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }

        hops.dedup_by(|a, b| a.edge == b.edge);
        Ok(hops)
    }
}
