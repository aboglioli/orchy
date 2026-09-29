use std::sync::Arc;

use orchy_core::{
    DocumentQuery, DocumentStore, DomainError, Id, MessageStore, TaskQuery, TaskStore,
};
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Task,
    Document,
    Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveReferenceCommand {
    pub kind: ReferenceKind,
    pub input: String,
}

/// What people type to point at an entity: a full id, an id prefix or suffix, or, for tasks
/// and documents, a fragment of the title. Every entity is considered, never a page of them,
/// and more than one match is refused rather than guessed.
pub struct ResolveReference {
    tasks: Arc<dyn TaskStore>,
    documents: Arc<dyn DocumentStore>,
    messages: Arc<dyn MessageStore>,
}

impl ResolveReference {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        documents: Arc<dyn DocumentStore>,
        messages: Arc<dyn MessageStore>,
    ) -> Self {
        Self {
            tasks,
            documents,
            messages,
        }
    }

    pub async fn execute(&self, cmd: ResolveReferenceCommand) -> ApplicationResult<String> {
        if let Ok(id) = Id::new(&cmd.input) {
            return Ok(id.to_string());
        }
        let needle = cmd.input.trim().to_lowercase();
        let mut candidates: Vec<String> = match cmd.kind {
            ReferenceKind::Task => self
                .tasks
                .matching(&TaskQuery::default())
                .await?
                .iter()
                .filter(|t| names(t.id(), Some(t.title().as_str()), &needle))
                .map(|t| t.id().to_string())
                .collect(),
            ReferenceKind::Document => self
                .documents
                .matching(&DocumentQuery::default())
                .await?
                .iter()
                .filter(|d| names(d.id(), Some(d.title().as_str()), &needle))
                .map(|d| d.id().to_string())
                .collect(),
            ReferenceKind::Message => self
                .messages
                .all()
                .await?
                .iter()
                .filter(|m| names(m.id(), None, &needle))
                .map(|m| m.id().to_string())
                .collect(),
        };
        candidates.sort();
        candidates.dedup();
        match candidates.len() {
            1 => Ok(candidates.remove(0)),
            0 => Err(DomainError::not_found(kind_name(cmd.kind), &cmd.input).into()),
            count => Err(DomainError::Ambiguous {
                input: cmd.input,
                count,
            }
            .into()),
        }
    }
}

fn names(id: &Id, title: Option<&str>, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let id = id.to_string().to_lowercase();
    id.starts_with(needle)
        || id.ends_with(needle)
        || title.is_some_and(|t| t.to_lowercase().contains(needle))
}

fn kind_name(kind: ReferenceKind) -> &'static str {
    match kind {
        ReferenceKind::Task => "task",
        ReferenceKind::Document => "document",
        ReferenceKind::Message => "message",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn a_prefix_a_suffix_and_a_title_fragment_all_name_an_entity() {
        let id = Id::new(A).unwrap();
        assert!(names(&id, Some("Rotate keys"), "01arz"));
        assert!(names(&id, Some("Rotate keys"), "g5fav"));
        assert!(names(&id, Some("Rotate keys"), "rotate"));
        assert!(!names(&id, Some("Rotate keys"), "zzz"));
    }

    #[test]
    fn a_message_is_never_matched_by_words() {
        assert!(!names(&Id::new(A).unwrap(), None, "rotate"));
    }
}
