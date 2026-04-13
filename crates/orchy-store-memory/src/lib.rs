mod agent;
mod context;
mod memory;
mod message;
mod project;
mod skill;
mod store_impl;
mod task;

use std::collections::HashMap;
use std::sync::RwLock;

use orchy_core::agent::{Agent, AgentId};
use orchy_core::memory::{ContextSnapshot, MemoryEntry, SnapshotId};
use orchy_core::message::{Message, MessageId};
use orchy_core::namespace::ProjectId;
use orchy_core::project::Project;
use orchy_core::skill::Skill;
use orchy_core::task::{Task, TaskId};

pub struct MemoryBackend {
    pub(crate) agents: RwLock<HashMap<AgentId, Agent>>,
    pub(crate) tasks: RwLock<HashMap<TaskId, Task>>,
    pub(crate) memory: RwLock<HashMap<(String, String), MemoryEntry>>,
    pub(crate) messages: RwLock<HashMap<MessageId, Message>>,
    pub(crate) contexts: RwLock<HashMap<SnapshotId, ContextSnapshot>>,
    pub(crate) skills: RwLock<HashMap<(String, String), Skill>>,
    pub(crate) projects: RwLock<HashMap<ProjectId, Project>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            memory: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            contexts: RwLock::new(HashMap::new()),
            skills: RwLock::new(HashMap::new()),
            projects: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}
