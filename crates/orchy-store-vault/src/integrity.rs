use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{EntityKind, Integrity, Problem, Result};

use crate::codec;
use crate::markdown::MarkdownFile;
use crate::vault::Vault;

pub struct VaultIntegrity {
    vault: Arc<Vault>,
}

impl VaultIntegrity {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl Integrity for VaultIntegrity {
    async fn unreadable(&self) -> Result<Vec<Problem>> {
        let scan = self.vault.scan().await?;
        let mut problems = scan.problems;
        for (_, located, file) in &scan.entries {
            if let Some(problem) = undecodable(&located.key, located.kind, file) {
                problems.push(problem);
            }
        }
        problems.sort_by(|a, b| a.location.cmp(&b.location));
        Ok(problems)
    }

    async fn problems(&self) -> Result<Vec<Problem>> {
        self.unreadable().await
    }

    async fn repair(&self, _problem: &Problem) -> Result<bool> {
        Ok(false)
    }
}

fn undecodable(key: &str, kind: EntityKind, file: &MarkdownFile) -> Option<Problem> {
    let decoded = match kind {
        EntityKind::Document => codec::document_from_markdown(file).map(drop),
        EntityKind::Task => codec::task_from_markdown(file).map(drop),
        EntityKind::Message => codec::message_from_markdown(file).map(drop),
        EntityKind::Skill => codec::skill_from_markdown(file).map(drop),
        EntityKind::Actor => codec::actor_from_markdown(file).map(drop),
    };
    decoded.err().map(|e| codec::problem(key, file, &e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::{BlobStore, MemoryBlobStore};
    use orchy_core::ProblemKind;

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    async fn integrity_over(files: &[(&str, &str)]) -> VaultIntegrity {
        let blobs = Arc::new(MemoryBlobStore::new());
        for (key, text) in files {
            blobs.put(key, text.as_bytes()).await.unwrap();
        }
        let vault = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        VaultIntegrity::new(Arc::new(vault))
    }

    async fn kinds(files: &[(&str, &str)]) -> Vec<(String, ProblemKind)> {
        integrity_over(files)
            .await
            .unreadable()
            .await
            .unwrap()
            .into_iter()
            .map(|p| (p.location, p.kind))
            .collect()
    }

    #[tokio::test]
    async fn invalid_yaml_is_reported_with_its_path_instead_of_failing() {
        let found = kinds(&[(
            "docs/x.md",
            &format!("---\nid: {A}\ntype: note\ntitle: [broken\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![("docs/x.md".to_owned(), ProblemKind::Unreadable)]
        );
    }

    #[tokio::test]
    async fn an_unknown_type_is_reported_as_such() {
        let found = kinds(&[(
            "docs/x.md",
            &format!("---\nid: {A}\ntype: brainstorm\ntitle: x\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![("docs/x.md".to_owned(), ProblemKind::UnknownType)]
        );
    }

    #[tokio::test]
    async fn a_skill_without_a_name_is_a_missing_field() {
        let found = kinds(&[(
            "docs/x.md",
            &format!("---\nid: {A}\ntype: skill\ntitle: x\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![("docs/x.md".to_owned(), ProblemKind::MissingField)]
        );
    }

    #[tokio::test]
    async fn merge_conflict_markers_make_a_file_unreadable() {
        let found = kinds(&[(
            "docs/x.md",
            &format!(
                "---\nid: {A}\ntype: note\n<<<<<<< HEAD\ntitle: ours\n=======\ntitle: theirs\n>>>>>>> branch\n---\n"
            ),
        )])
        .await;
        assert_eq!(
            found,
            vec![("docs/x.md".to_owned(), ProblemKind::Unreadable)]
        );
    }

    #[tokio::test]
    async fn two_files_sharing_an_id_are_a_duplicate() {
        let note = format!("---\nid: {A}\ntype: note\ntitle: x\n---\n");
        let found = kinds(&[("docs/a.md", &note), ("docs/b.md", &note)]).await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, ProblemKind::DuplicateId);
    }

    #[tokio::test]
    async fn a_healthy_vault_has_nothing_to_report() {
        let found = kinds(&[
            (
                "docs/a.md",
                &format!("---\nid: {A}\ntype: note\ntitle: a\n---\n"),
            ),
            (
                "skills/b.md",
                &format!("---\nid: {B}\ntype: skill\nname: commits\nsummary: s\n---\n"),
            ),
        ])
        .await;
        assert!(found.is_empty(), "{found:?}");
    }
}
