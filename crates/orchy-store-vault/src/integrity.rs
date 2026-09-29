use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{EntityKind, EntityRef, Id, Integrity, Problem, ProblemKind, Relation, Result};

use crate::codec;
use crate::markdown::MarkdownFile;
use crate::vault::Vault;
use crate::vault::refs_in;

pub struct VaultIntegrity {
    vault: Arc<Vault>,
}

impl VaultIntegrity {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    fn placement(
        &self,
        id: &Id,
        key: &str,
        kind: EntityKind,
        file: &MarkdownFile,
    ) -> Option<Problem> {
        let expected = self.expected_key(id, kind, file)?;
        if expected == key {
            return None;
        }
        let folder = |k: &str| {
            k.rsplit_once('/')
                .map(|(f, _)| f.to_owned())
                .unwrap_or_default()
        };
        let problem = if folder(&expected) == folder(key) {
            ProblemKind::MisnamedFile
        } else {
            ProblemKind::Misplaced
        };
        Some(Problem::new(
            problem,
            key,
            Some(id.clone()),
            format!("belongs at `{expected}`"),
        ))
    }

    /// Where the file should be. A document may sit anywhere under its namespace folder, so
    /// only its name is fixed there.
    fn expected_key(&self, id: &Id, kind: EntityKind, file: &MarkdownFile) -> Option<String> {
        let layout = self.vault.layout();
        match kind {
            EntityKind::Document => {
                let document = codec::document_from_markdown(file).ok()?;
                let key = self.vault.locate(id)?.key;
                let name = format!("{id}.md");
                let folder = layout.document_folder(document.namespace());
                if !key.starts_with(&folder) {
                    return Some(layout.document_key(document.namespace(), id));
                }
                let current_folder = key.rsplit_once('/').map_or("", |(f, _)| f);
                Some(format!("{current_folder}/{name}"))
            }
            EntityKind::Task => {
                let task = codec::task_from_markdown(file).ok()?;
                Some(layout.task_key(id, task.status()))
            }
            EntityKind::Message => {
                let message = codec::message_from_markdown(file).ok()?;
                Some(layout.message_key(message.thread(), id))
            }
            EntityKind::Skill => {
                let skill = codec::skill_from_markdown(file).ok()?;
                Some(layout.skill_key(skill.namespace(), skill.name()))
            }
            EntityKind::Actor => None,
        }
    }

    fn dangling(&self, key: &str, id: &Id, file: &MarkdownFile) -> Vec<Problem> {
        let mut problems = Vec::new();
        for (field, value) in file.frontmatter.iter() {
            let Ok(relation) = field.parse::<Relation>() else {
                continue;
            };
            for target in refs_in(value) {
                let Ok(to) = EntityRef::parse_or_assume(&target, relation.sole_target_kind())
                else {
                    continue;
                };
                if to.kind() == EntityKind::Actor || self.vault.locate(to.id()).is_some() {
                    continue;
                }
                problems.push(Problem::new(
                    ProblemKind::DanglingEdge,
                    key,
                    Some(id.clone()),
                    format!("`{relation}` points at {to}, which does not exist"),
                ));
            }
        }
        problems
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
        let mut problems = self.unreadable().await?;
        let scan = self.vault.scan().await?;
        for (id, located, file) in &scan.entries {
            problems.extend(self.placement(id, &located.key, located.kind, file));
            problems.extend(self.dangling(&located.key, id, file));
        }
        problems.sort_by(|a, b| a.location.cmp(&b.location).then(a.kind.cmp(&b.kind)));
        Ok(problems)
    }

    async fn repair(&self, problem: &Problem) -> Result<bool> {
        if !matches!(
            problem.kind,
            ProblemKind::Misplaced | ProblemKind::MisnamedFile
        ) {
            return Ok(false);
        }
        let Some(id) = &problem.id else {
            return Ok(false);
        };
        let Some((key, file)) = self.vault.read_by_id(id).await? else {
            return Ok(false);
        };
        let Some(kind) = self.vault.locate(id).map(|l| l.kind) else {
            return Ok(false);
        };
        let Some(expected) = self.expected_key(id, kind, &file) else {
            return Ok(false);
        };
        if expected == key {
            return Ok(true);
        }
        self.vault.relocate(id, &expected).await?;
        Ok(true)
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

    async fn everything(files: &[(&str, &str)]) -> (VaultIntegrity, Vec<(String, ProblemKind)>) {
        let integrity = integrity_over(files).await;
        let found = integrity
            .problems()
            .await
            .unwrap()
            .into_iter()
            .map(|p| (p.location, p.kind))
            .collect();
        (integrity, found)
    }

    #[tokio::test]
    async fn a_task_outside_its_status_folder_is_misplaced_and_moved_back() {
        let (integrity, found) = everything(&[(
            "tasks/done/x.md",
            &format!("---\nid: {A}\ntype: task\ntitle: t\nstatus: pending\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![("tasks/done/x.md".to_owned(), ProblemKind::Misplaced)]
        );

        let problem = integrity.problems().await.unwrap().remove(0);
        assert!(integrity.repair(&problem).await.unwrap());
        assert!(integrity.problems().await.unwrap().is_empty());
        assert!(
            integrity
                .vault
                .locate(&Id::new(A).unwrap())
                .unwrap()
                .key
                .ends_with(&format!("tasks/open/{A}.md"))
        );
    }

    #[tokio::test]
    async fn a_document_filed_by_hand_keeps_its_folder_but_takes_its_id_as_name() {
        let (integrity, found) = everything(&[(
            "docs/backend/notes/mine.md",
            &format!("---\nid: {A}\ntype: note\ntitle: t\nnamespace: /backend\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![(
                "docs/backend/notes/mine.md".to_owned(),
                ProblemKind::MisnamedFile
            )]
        );
        let problem = integrity.problems().await.unwrap().remove(0);
        integrity.repair(&problem).await.unwrap();
        assert_eq!(
            integrity.vault.locate(&Id::new(A).unwrap()).unwrap().key,
            format!("docs/backend/notes/{A}.md")
        );
    }

    #[tokio::test]
    async fn a_link_to_nothing_is_reported_but_left_alone() {
        let (integrity, found) = everything(&[(
            &format!("docs/{A}.md"),
            &format!("---\nid: {A}\ntype: note\ntitle: t\nrelated_to:\n  - document:{B}\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![(format!("docs/{A}.md"), ProblemKind::DanglingEdge)]
        );
        let problem = integrity.problems().await.unwrap().remove(0);
        assert!(!integrity.repair(&problem).await.unwrap());
    }

    #[tokio::test]
    async fn a_healthy_vault_has_nothing_to_report() {
        let note = format!("docs/{A}.md");
        let (_, found) = everything(&[
            (&note, &format!("---\nid: {A}\ntype: note\ntitle: a\n---\n")),
            (
                "skills/commits.md",
                &format!("---\nid: {B}\ntype: skill\nname: commits\nsummary: s\n---\n"),
            ),
        ])
        .await;
        assert!(found.is_empty(), "{found:?}");
    }
}
