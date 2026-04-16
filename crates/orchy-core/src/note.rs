use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::AgentId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    author: Option<AgentId>,
    body: String,
    created_at: DateTime<Utc>,
}

impl Note {
    pub fn new(author: Option<AgentId>, body: String) -> Self {
        Self {
            author,
            body,
            created_at: Utc::now(),
        }
    }

    pub fn restore(author: Option<AgentId>, body: String, created_at: DateTime<Utc>) -> Self {
        Self {
            author,
            body,
            created_at,
        }
    }

    pub fn author(&self) -> Option<&AgentId> {
        self.author.as_ref()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}
