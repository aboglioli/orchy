use chrono::{DateTime, Utc};
use eventuary::{Payload, Topic};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::event::{DomainEvent, payload_of, topic};
use crate::id::Id;
use crate::namespace::Namespace;

macro_rules! skill_event {
    ($name:ident, $topic:literal) => {
        impl DomainEvent for $name {
            fn topic(&self) -> Topic {
                topic($topic)
            }

            fn key(&self) -> Id {
                self.id.clone()
            }

            fn namespace(&self) -> Namespace {
                self.namespace.clone()
            }

            fn payload(&self) -> Result<Payload> {
                payload_of(self)
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCreated {
    pub id: Id,
    pub namespace: Namespace,
    pub name: String,
    pub summary: String,
    pub at: DateTime<Utc>,
}
skill_event!(SkillCreated, "skill.created");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillWritten {
    pub id: Id,
    pub namespace: Namespace,
    pub name: String,
    pub at: DateTime<Utc>,
}
skill_event!(SkillWritten, "skill.written");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRenamed {
    pub id: Id,
    pub namespace: Namespace,
    pub from: String,
    pub to: String,
    pub at: DateTime<Utc>,
}
skill_event!(SkillRenamed, "skill.renamed");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMoved {
    pub id: Id,
    pub namespace: Namespace,
    pub from: Namespace,
    pub name: String,
    pub at: DateTime<Utc>,
}
skill_event!(SkillMoved, "skill.moved");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRetired {
    pub id: Id,
    pub namespace: Namespace,
    pub name: String,
    pub at: DateTime<Utc>,
}
skill_event!(SkillRetired, "skill.retired");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRestored {
    pub id: Id,
    pub namespace: Namespace,
    pub name: String,
    pub at: DateTime<Utc>,
}
skill_event!(SkillRestored, "skill.restored");
