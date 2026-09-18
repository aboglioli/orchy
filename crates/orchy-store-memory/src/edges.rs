use std::collections::BTreeSet;
use std::sync::Mutex;

use async_trait::async_trait;
use orchy_core::{Edge, EdgeStore, EntityRef, RelationType, Result, TraversalHop};

#[derive(Default)]
pub struct MemoryEdgeStore(Mutex<Vec<Edge>>);

impl MemoryEdgeStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn all(&self) -> Vec<Edge> {
        self.0.lock().expect("edge mutex").clone()
    }
}

#[async_trait]
impl EdgeStore for MemoryEdgeStore {
    async fn add(&self, edge: &Edge) -> Result<()> {
        let mut edges = self.0.lock().expect("edge mutex");
        if !edges.contains(edge) {
            edges.push(edge.clone());
        }
        Ok(())
    }

    async fn remove(&self, edge: &Edge) -> Result<()> {
        self.0.lock().expect("edge mutex").retain(|e| e != edge);
        Ok(())
    }

    async fn out(&self, from: &EntityRef, relation: Option<&RelationType>) -> Result<Vec<Edge>> {
        Ok(self
            .all()
            .into_iter()
            .filter(|e| e.from() == from)
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn incoming(&self, to: &EntityRef, relation: Option<&RelationType>) -> Result<Vec<Edge>> {
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
