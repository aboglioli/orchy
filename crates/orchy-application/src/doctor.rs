use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use orchy_core::task::rollup;
use orchy_core::{
    DocumentQuery, DocumentStatus, DocumentStore, Edge, EdgeStore, EntityKind, EntityRef, Id,
    Integrity, Problem, ProblemKind, Relation, Task, TaskQuery, TaskStatus, TaskStore, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;
use crate::task_graph::waits_of;
use crate::unit_of_work::atomically;

const FIX_PASSES: usize = 4;

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
        let found = self.examine().await?;
        if !cmd.fix {
            return Ok(DoctorDto {
                problems: found,
                fixed: 0,
            });
        }

        let mut fixed = 0;
        let mut remaining = found;
        for _ in 0..FIX_PASSES {
            let mut progressed = false;
            for problem in remaining.iter().filter(|p| p.fixable) {
                if let Ok(true) = atomically(&*self.unit_of_work, || self.repair(problem)).await {
                    fixed += 1;
                    progressed = true;
                }
            }
            remaining = self.examine().await?;
            if !progressed || !remaining.iter().any(|p| p.fixable) {
                break;
            }
        }
        Ok(DoctorDto {
            problems: remaining,
            fixed,
        })
    }

    async fn examine(&self) -> ApplicationResult<Vec<Problem>> {
        let mut problems = self.integrity.problems().await?;
        let tasks = self.tasks.matching(&TaskQuery::default()).await?;
        let supersedes = self.edges.of_relation(&Relation::Supersedes).await?;
        let parent_loops = cycles(&tasks);
        problems.extend(wait_loops(&tasks, &supersedes, &parent_loops));
        problems.extend(parent_loops);
        problems.extend(open_under_finished(&tasks));
        problems.extend(self.stale_rollups(&tasks));
        problems.extend(self.missing_successors(&tasks, &supersedes).await?);
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

    async fn missing_successors(
        &self,
        tasks: &[Task],
        supersedes: &[Edge],
    ) -> ApplicationResult<Vec<Problem>> {
        let replaced: HashSet<EntityRef> = supersedes.iter().map(|e| e.to().clone()).collect();
        let derived: HashSet<EntityRef> = self
            .edges
            .of_relation(&Relation::DerivedFrom)
            .await?
            .into_iter()
            .filter(|e| e.from().kind() == EntityKind::Skill)
            .map(|e| e.to().clone())
            .collect();

        let mut problems = Vec::new();
        let mut report = |entity: EntityRef, id: &Id, detail: &str| {
            problems.push(Problem::new(
                ProblemKind::MissingSuccessor,
                entity.to_string(),
                Some(id.clone()),
                detail,
            ));
        };
        for task in tasks
            .iter()
            .filter(|t| t.status() == TaskStatus::Superseded)
        {
            let entity = EntityRef::task(task.id().clone());
            if !replaced.contains(&entity) {
                report(
                    entity,
                    task.id(),
                    "is superseded, but no task records replacing it",
                );
            }
        }
        let retired = self
            .documents
            .matching(&DocumentQuery {
                status: Some(vec![DocumentStatus::Superseded, DocumentStatus::Promoted]),
                ..Default::default()
            })
            .await?;
        for document in retired {
            let entity = EntityRef::document(document.id().clone());
            match document.status() {
                Some(DocumentStatus::Superseded) if !replaced.contains(&entity) => report(
                    entity,
                    document.id(),
                    "is superseded, but no document records replacing it",
                ),
                Some(DocumentStatus::Promoted) if !derived.contains(&entity) => report(
                    entity,
                    document.id(),
                    "is promoted, but no skill records being derived from it",
                ),
                _ => {}
            }
        }
        Ok(problems)
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

fn wait_loops(tasks: &[Task], supersedes: &[Edge], parent_loops: &[Problem]) -> Vec<Problem> {
    let in_parent_loop: HashSet<&Id> = parent_loops.iter().filter_map(|p| p.id.as_ref()).collect();
    let parent_of: HashMap<&Id, &Id> = tasks
        .iter()
        .filter_map(|t| t.parent().map(|p| (t.id(), p)))
        .collect();
    let mut problems = Vec::new();
    for cycle in waits_of(tasks, supersedes).loops() {
        let only_parents = cycle
            .iter()
            .zip(cycle.iter().cycle().skip(1))
            .all(|(waiter, on)| parent_of.get(on) == Some(&waiter));
        if only_parents && cycle.iter().any(|id| in_parent_loop.contains(id)) {
            continue;
        }
        let first = cycle[0].clone();
        let walk: Vec<String> = cycle.iter().map(ToString::to_string).collect();
        problems.push(Problem::new(
            ProblemKind::WaitLoop,
            EntityRef::task(first.clone()).to_string(),
            Some(first),
            format!(
                "waits on itself ({} waits on {}), so none of it can ever finish",
                walk.join(" waits on "),
                walk[0]
            ),
        ));
    }
    problems
}

fn open_under_finished(tasks: &[Task]) -> Vec<Problem> {
    let status_of: HashMap<&Id, TaskStatus> = tasks.iter().map(|t| (t.id(), t.status())).collect();
    tasks
        .iter()
        .filter(|t| !t.status().is_terminal())
        .filter_map(|child| {
            let parent = child.parent()?;
            let status = status_of.get(parent)?;
            status.is_terminal().then(|| {
                Problem::new(
                    ProblemKind::OpenUnderFinished,
                    EntityRef::task(child.id().clone()).to_string(),
                    Some(child.id().clone()),
                    format!("is {} beneath {parent}, which is {status}", child.status()),
                )
            })
        })
        .collect()
}
