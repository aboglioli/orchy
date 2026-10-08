use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    Edge, EntityKind, EntityRef, Id, Integrity, Kind, Problem, ProblemKind, Relation, Result,
};
use serde_json::Value;

use crate::edges::edges_in;

use crate::codec;
use crate::layout::AGENTS;
use crate::markdown::MarkdownFile;
use crate::placement;
use crate::vault::refs_in;
use crate::vault::{Scan, Vault};

pub struct VaultIntegrity {
    vault: Arc<Vault>,
}

impl VaultIntegrity {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    async fn placement(
        &self,
        id: &Id,
        key: &str,
        kind: EntityKind,
        file: &MarkdownFile,
    ) -> Result<Option<Problem>> {
        let Some(expected) = self.expected_key(kind, file).await? else {
            return Ok(None);
        };
        if expected == key {
            return Ok(None);
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
        Ok(Some(Problem::new(
            problem,
            key,
            Some(id.clone()),
            format!("belongs at `{expected}`"),
        )))
    }

    async fn expected_key(&self, kind: EntityKind, file: &MarkdownFile) -> Result<Option<String>> {
        let vault = &self.vault;
        let key = match kind {
            EntityKind::Document => match codec::document_from_markdown(file) {
                Ok(document) => placement::of_document(vault, &document).await?,
                Err(_) => return Ok(None),
            },
            EntityKind::Task => match codec::task_from_markdown(file) {
                Ok(task) => placement::of_task(vault, &task).await?,
                Err(_) => return Ok(None),
            },
            EntityKind::Message => match codec::message_from_markdown(file) {
                Ok(message) => placement::of_message(vault, &message).await?,
                Err(_) => return Ok(None),
            },
            EntityKind::Skill => match codec::skill_from_markdown(file) {
                Ok(skill) => placement::of_skill(vault, &skill).await?,
                Err(_) => return Ok(None),
            },
            EntityKind::Actor => return Ok(None),
        };
        Ok(Some(key))
    }

    fn projections(&self, scan: &Scan) -> BTreeMap<(Id, &'static str), BTreeSet<String>> {
        let mut expected: BTreeMap<(Id, &'static str), BTreeSet<String>> = BTreeMap::new();
        for (id, located, file) in &scan.entries {
            if located.kind == EntityKind::Actor {
                continue;
            }
            let source = EntityRef::new(located.kind, id.clone());
            for edge in edges_in(&source, file) {
                let inverse = edge.relation().inverse();
                if !Kind::is_projected_field(inverse) {
                    continue;
                }
                let Some(target) = edge.to().id() else {
                    continue;
                };
                expected
                    .entry((target.clone(), inverse))
                    .or_default()
                    .insert(edge.from().to_string());
            }
        }
        expected
    }

    fn stale_projections(
        &self,
        scan: &Scan,
        expected: &BTreeMap<(Id, &'static str), BTreeSet<String>>,
    ) -> Vec<Problem> {
        let mut problems = Vec::new();
        for (id, located, file) in &scan.entries {
            for field in Kind::PROJECTED_FIELDS {
                let shown: BTreeSet<String> =
                    refs_in(file.frontmatter.get(field).unwrap_or(&Value::Null))
                        .into_iter()
                        .collect();
                let wanted = expected
                    .get(&(id.clone(), field))
                    .cloned()
                    .unwrap_or_default();
                if shown == wanted {
                    continue;
                }
                let list = |refs: &BTreeSet<String>| {
                    if refs.is_empty() {
                        return "nothing".to_owned();
                    }
                    refs.iter().cloned().collect::<Vec<_>>().join(", ")
                };
                problems.push(Problem::new(
                    ProblemKind::StaleProjection,
                    &located.key,
                    Some(id.clone()),
                    format!(
                        "`{field}` shows {} but the links stored elsewhere say {}",
                        list(&shown),
                        list(&wanted)
                    ),
                ));
            }
        }
        problems
    }

    async fn reproject(&self, id: &Id) -> Result<bool> {
        let scan = self.vault.scan().await?;
        let expected = self.projections(&scan);
        for field in Kind::PROJECTED_FIELDS {
            let wanted: Vec<String> = expected
                .get(&(id.clone(), field))
                .map(|refs| refs.iter().cloned().collect())
                .unwrap_or_default();
            self.vault
                .amend_refs(id, field, move |refs| *refs = wanted.clone())
                .await?;
        }
        Ok(true)
    }

    fn links(
        &self,
        source: (&str, &Id, EntityKind),
        file: &MarkdownFile,
        roster: &HashSet<String>,
    ) -> Vec<Problem> {
        let (key, id, kind) = source;
        let mut problems = Vec::new();
        let invalid =
            |detail: String| Problem::new(ProblemKind::InvalidLink, key, Some(id.clone()), detail);
        for (field, value) in file.frontmatter.iter() {
            let Ok(relation) = field.parse::<Relation>() else {
                continue;
            };
            for target in refs_in(value) {
                let to = match EntityRef::parse_or_assume(&target, relation.sole_target_kind()) {
                    Ok(to) => to,
                    Err(e) => {
                        problems.push(invalid(format!("`{relation}` holds `{target}`: {e}")));
                        continue;
                    }
                };
                if let Err(e) = Edge::new(EntityRef::new(kind, id.clone()), to.clone(), relation) {
                    problems.push(invalid(e.to_string()));
                    continue;
                }
                let found = match (to.id(), to.as_actor()) {
                    (Some(target), _) => self.vault.locate(target).map(|l| l.kind),
                    (_, Some(actor)) => roster
                        .contains(&self.vault.layout().actor_key(actor))
                        .then_some(EntityKind::Actor),
                    _ => None,
                };
                match found {
                    None => problems.push(Problem::new(
                        ProblemKind::DanglingEdge,
                        key,
                        Some(id.clone()),
                        format!("`{relation}` points at {to}, which does not exist"),
                    )),
                    Some(actual) if actual != to.kind() => problems.push(invalid(format!(
                        "`{relation}` points at {to}, which is a {actual}"
                    ))),
                    Some(_) => {}
                }
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
        let roster: HashSet<String> = self.vault.blobs().list(AGENTS).await?.into_iter().collect();
        for (id, located, file) in &scan.entries {
            problems.extend(self.placement(id, &located.key, located.kind, file).await?);
            problems.extend(self.links((&located.key, id, located.kind), file, &roster));
        }
        let expected = self.projections(&scan);
        problems.extend(self.stale_projections(&scan, &expected));
        problems.sort_by(|a, b| a.location.cmp(&b.location).then(a.kind.cmp(&b.kind)));
        Ok(problems)
    }

    async fn repair(&self, problem: &Problem) -> Result<bool> {
        if let (ProblemKind::StaleProjection, Some(id)) = (problem.kind, &problem.id) {
            return self.reproject(id).await;
        }
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
        let Some(expected) = self.expected_key(kind, &file).await? else {
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
                .ends_with("tasks/open/t.md")
        );
    }

    #[tokio::test]
    async fn a_document_in_a_folder_its_namespace_does_not_name_is_moved_to_its_own() {
        let (integrity, found) = everything(&[(
            "docs/backend/notes/mine.md",
            &format!("---\nid: {A}\ntype: note\ntitle: t\nnamespace: /backend\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![(
                "docs/backend/notes/mine.md".to_owned(),
                ProblemKind::Misplaced
            )]
        );
        let problem = integrity.problems().await.unwrap().remove(0);
        integrity.repair(&problem).await.unwrap();
        assert_eq!(
            integrity.vault.locate(&Id::new(A).unwrap()).unwrap().key,
            "docs/backend/t.md"
        );
    }

    #[tokio::test]
    async fn a_document_in_its_own_folder_under_another_name_is_renamed() {
        let (_, found) = everything(&[(
            "docs/backend/mine.md",
            &format!("---\nid: {A}\ntype: note\ntitle: t\nnamespace: /backend\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![("docs/backend/mine.md".to_owned(), ProblemKind::MisnamedFile)]
        );
    }

    #[tokio::test]
    async fn a_link_to_nothing_is_reported_but_left_alone() {
        let (integrity, found) = everything(&[(
            "docs/t.md",
            &format!("---\nid: {A}\ntype: note\ntitle: t\nrelated_to:\n  - document:{B}\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![("docs/t.md".to_owned(), ProblemKind::DanglingEdge)]
        );
        let problem = integrity.problems().await.unwrap().remove(0);
        assert!(!integrity.repair(&problem).await.unwrap());
    }

    #[tokio::test]
    async fn a_file_named_by_its_id_is_renamed_after_its_title() {
        let (integrity, found) = everything(&[(
            &format!("docs/{A}.md"),
            &format!("---\nid: {A}\ntype: note\ntitle: Rotate keys\n---\n"),
        )])
        .await;
        assert_eq!(
            found,
            vec![(format!("docs/{A}.md"), ProblemKind::MisnamedFile)]
        );
        let problem = integrity.problems().await.unwrap().remove(0);
        assert!(integrity.repair(&problem).await.unwrap());
        assert_eq!(
            integrity.vault.locate(&Id::new(A).unwrap()).unwrap().key,
            "docs/rotate-keys.md"
        );
    }

    #[tokio::test]
    async fn two_titles_alike_take_numbered_names_and_keep_them() {
        let (_, found) = everything(&[
            (
                "docs/plan.md",
                &format!("---\nid: {A}\ntype: note\ntitle: Plan\n---\n"),
            ),
            (
                "docs/plan-2.md",
                &format!("---\nid: {B}\ntype: note\ntitle: Plan\n---\n"),
            ),
        ])
        .await;
        assert!(found.is_empty(), "{found:?}");
    }

    #[tokio::test]
    async fn a_healthy_vault_has_nothing_to_report() {
        let (_, found) = everything(&[
            (
                "docs/a.md",
                &format!("---\nid: {A}\ntype: note\ntitle: a\n---\n"),
            ),
            (
                "skills/commits.md",
                &format!("---\nid: {B}\ntype: skill\nname: commits\nsummary: s\n---\n"),
            ),
        ])
        .await;
        assert!(found.is_empty(), "{found:?}");
    }
}
