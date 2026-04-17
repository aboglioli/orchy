use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Task,
    Knowledge,
    Agent,
    Message,
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceKind::Task => write!(f, "task"),
            ResourceKind::Knowledge => write!(f, "knowledge"),
            ResourceKind::Agent => write!(f, "agent"),
            ResourceKind::Message => write!(f, "message"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    kind: ResourceKind,
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display: Option<String>,
}

impl ResourceRef {
    pub fn new(kind: ResourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            display: None,
        }
    }

    pub fn with_display(mut self, display: impl Into<String>) -> Self {
        self.display = Some(display.into());
        self
    }

    pub fn task(id: impl Into<String>) -> Self {
        Self::new(ResourceKind::Task, id)
    }

    pub fn knowledge(id: impl Into<String>) -> Self {
        Self::new(ResourceKind::Knowledge, id)
    }

    pub fn agent(id: impl Into<String>) -> Self {
        Self::new(ResourceKind::Agent, id)
    }

    pub fn message(id: impl Into<String>) -> Self {
        Self::new(ResourceKind::Message, id)
    }

    pub fn kind(&self) -> &ResourceKind {
        &self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}
