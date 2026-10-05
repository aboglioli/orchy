use std::collections::HashSet;
use std::sync::Arc;

use orchy_core::task::rollup;
use orchy_core::{
    DocumentQuery, DocumentStatus, DocumentStore, Edge, EdgeStore, EntityRef, Id, Integrity,
    Problem, ProblemKind, Relation, Task, TaskQuery, TaskStore, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DoctorCommand {
    pub fix: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorDto {
    pub problems: Vec<Problem>,
    pub fixed: usize,
}

pub struct Doctor {
    integrity: Arc<dyn Integrity>,
    tasks: Arc<dyn TaskStore>,
    documents: Arc<dyn DocumentStore>,
    edges: Arc<dyn EdgeStore>,
    rollup: Arc<RollupAncestors>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl Doctor {
    pub fn new(
        integrity: Arc<dyn Integrity>,
        tasks: Arc<dyn TaskStore>,
        documents: Arc<dyn DocumentStore>,
        edges: Arc<dyn EdgeStore>,
        rollup: Arc<RollupAncestors>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            integrity,
            tasks,
            documents,
            edges,
            rollup,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: DoctorCommand) -> ApplicationResult<DoctorDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: DoctorCommand) -> ApplicationResult<DoctorDto> {
        let found = self.examine().await?;
        if !cmd.fix {
            return Ok(DoctorDto {
                problems: found,
                fixed: 0,
            });
        }

        let mut fixed = 0;
        for problem in found.iter().filter(|p| p.fixable) {
            if self.repair(problem).await? {
                fixed += 1;
            }
        }
        Ok(DoctorDto {
            problems: self.examine().await?,
            fixed,
        })
    }

    async fn examine(&self) -> ApplicationResult<Vec<Problem>> {
        let mut problems = self.integrity.problems().await?;
        let tasks = self.tasks.matching(&TaskQuery::default()).await?;
        problems.extend(cycles(&tasks));
        problems.extend(self.stale_rollups(&tasks));
        problems.extend(self.inverted_supersedes().await?);
        Ok(problems)
    }

    async fn repair(&self, problem: &Problem) -> ApplicationResult<bool> {
        match (problem.kind, &problem.id) {
            (ProblemKind::StaleRollup, Some(parent)) => {
                self.rollup.from_parent(parent).await?;
                Ok(true)
            }
            (ProblemKind::InvertedSupersedes, Some(old)) => self.turn_around(old).await,
            _ => Ok(self.integrity.repair(problem).await?),
        }
    }

    fn stale_rollups(&self, tasks: &[Task]) -> Vec<Problem> {
        let mut problems = Vec::new();
        for parent in tasks.iter().filter(|t| !t.status().is_terminal()) {
            let statuses: Vec<_> = tasks
                .iter()
                .filter(|t| t.parent() == Some(parent.id()))
                .map(Task::status)
                .collect();
            if let Some(derived) = rollup::resolve(&statuses) {
                problems.push(Problem::new(
                    ProblemKind::StaleRollup,
                    EntityRef::task(parent.id().clone()).to_string(),
                    Some(parent.id().clone()),
                    format!(
                        "is {} but every subtask is finished, so it should be {derived}",
                        parent.status()
                    ),
                ));
            }
        }
        problems
    }

    /// Older versions stored `supersedes` on the replaced document, pointing at its
    /// replacement: a superseded document that supersedes a live one is that inversion.
    async fn inverted_supersedes(&self) -> ApplicationResult<Vec<Problem>> {
        let superseded = self
            .documents
            .matching(&DocumentQuery {
                status: Some(vec![DocumentStatus::Superseded]),
                ..Default::default()
            })
            .await?;
        let mut problems = Vec::new();
        for old in superseded {
            for edge in self.inverted_from(old.id()).await? {
                problems.push(Problem::new(
                    ProblemKind::InvertedSupersedes,
                    EntityRef::document(old.id().clone()).to_string(),
                    Some(old.id().clone()),
                    format!(
                        "records that it supersedes {}; the link belongs on that document",
                        edge.to()
                    ),
                ));
            }
        }
        Ok(problems)
    }

    async fn inverted_from(&self, old: &Id) -> ApplicationResult<Vec<Edge>> {
        let edges = self
            .edges
            .out(
                &EntityRef::document(old.clone()),
                Some(&Relation::Supersedes),
            )
            .await?;
        let mut inverted = Vec::new();
        for edge in edges {
            let Some(target) = edge.to().id() else {
                continue;
            };
            let target = self.documents.get(target).await?;
            if target.is_some_and(|t| t.status() != Some(DocumentStatus::Superseded)) {
                inverted.push(edge);
            }
        }
        Ok(inverted)
    }

    async fn turn_around(&self, old: &Id) -> ApplicationResult<bool> {
        let inverted = self.inverted_from(old).await?;
        for edge in &inverted {
            self.edges
                .add(&Edge::new(
                    edge.to().clone(),
                    edge.from().clone(),
                    Relation::Supersedes,
                )?)
                .await?;
            self.edges.remove(edge).await?;
        }
        Ok(!inverted.is_empty())
    }
}

fn cycles(tasks: &[Task]) -> Vec<Problem> {
    let parent_of = |id: &Id| {
        tasks
            .iter()
            .find(|t| t.id() == id)
            .and_then(|t| t.parent().cloned())
    };
    let mut reported = HashSet::new();
    let mut problems = Vec::new();
    for task in tasks {
        let mut path = vec![task.id().clone()];
        let mut cursor = task.parent().cloned();
        while let Some(id) = cursor {
            if id == *task.id() {
                if path.iter().all(|member| reported.insert(member.clone())) {
                    problems.push(Problem::new(
                        ProblemKind::ParentCycle,
                        EntityRef::task(task.id().clone()).to_string(),
                        Some(task.id().clone()),
                        format!(
                            "is its own ancestor through {}",
                            path.iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(" → ")
                        ),
                    ));
                }
                break;
            }
            if path.contains(&id) || path.len() > rollup::MAX_DEPTH {
                break;
            }
            path.push(id.clone());
            cursor = parent_of(&id);
        }
    }
    problems
}
