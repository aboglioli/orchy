use std::collections::{HashMap, HashSet};
use std::result::Result as StdResult;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use orchy_core::{DomainError, EntityKind, Id, Problem, ProblemKind, Result};
use tokio::time::sleep;

use serde_json::Value;

use crate::blob::{BlobStore, digest};
use crate::codec;
use crate::layout::Layout;
use crate::markdown::MarkdownFile;

const LOOKUP_ATTEMPTS: u32 = 4;
const AMEND_ATTEMPTS: u32 = 16;
const RESCAN_PASSES: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amended {
    Missing,
    Unchanged,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precondition {
    Any,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub key: String,
    pub kind: EntityKind,
    pub seen: u64,
}

pub struct Scan {
    pub entries: Vec<(Id, Located, Arc<MarkdownFile>)>,
    pub problems: Vec<Problem>,
}

pub struct Vault {
    blobs: Arc<dyn BlobStore>,
    layout: Layout,
    index: RwLock<HashMap<Id, Located>>,
    problems: RwLock<Vec<Problem>>,
    /// Only a file whose fingerprint proves it unchanged is skipped: every scan must see
    /// what other agents wrote since the last one.
    parsed: RwLock<HashMap<String, Parsed>>,
}

struct Parsed {
    print: Option<u64>,
    digest: u64,
    file: Arc<MarkdownFile>,
}

impl Vault {
    pub async fn open(blobs: Arc<dyn BlobStore>) -> Result<Self> {
        let vault = Self {
            blobs,
            layout: Layout,
            index: RwLock::new(HashMap::new()),
            problems: RwLock::new(Vec::new()),
            parsed: RwLock::new(HashMap::new()),
        };
        vault.reindex().await?;
        Ok(vault)
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn blobs(&self) -> &Arc<dyn BlobStore> {
        &self.blobs
    }

    pub async fn reindex(&self) -> Result<()> {
        self.scan().await?;
        Ok(())
    }

    pub fn scan_problems(&self) -> Vec<Problem> {
        self.problems.read().expect("problems lock").clone()
    }

    pub async fn scan(&self) -> Result<Scan> {
        let mut found = Vec::new();
        let mut problems = Vec::new();
        let mut owner_of: HashMap<Id, String> = HashMap::new();
        let mut seen_keys = HashSet::new();
        let mut batch = self.blobs.list_fingerprinted("").await?;

        for _ in 0..RESCAN_PASSES {
            let wanted: Vec<(String, Option<u64>)> = batch
                .into_iter()
                .filter(|(key, _)| seen_keys.insert(key.clone()))
                .filter(|(key, _)| self.layout.is_markdown(key) && !self.layout.is_runtime(key))
                .collect();
            let (parsed, vanished) = self.parse_all(wanted).await?;
            for (key, seen, decoded) in parsed {
                let file = match decoded {
                    Ok(file) => file,
                    Err(detail) => {
                        problems.push(Problem::new(ProblemKind::Unreadable, &key, None, detail));
                        continue;
                    }
                };
                let Some(raw) = codec::id_of(&file) else {
                    continue;
                };
                let Ok(id) = Id::new(raw) else {
                    if codec::kind_of(&file) == Some("agent") {
                        continue;
                    }
                    problems.push(Problem::new(
                        ProblemKind::InvalidField,
                        &key,
                        None,
                        format!("`id: {raw}` is not a ULID"),
                    ));
                    continue;
                };
                if let Some(owner) = owner_of.get(&id) {
                    problems.push(Problem::new(
                        ProblemKind::DuplicateId,
                        &key,
                        Some(id.clone()),
                        format!("`{id}` is also the id of `{owner}`"),
                    ));
                    continue;
                }
                owner_of.insert(id.clone(), key.clone());
                let kind = kind_from(codec::kind_of(&file));
                found.push((id, Located { kind, key, seen }, file));
            }

            // a file that vanished between listing and reading was moved: look for where it went
            if !vanished {
                break;
            }
            batch = self
                .blobs
                .list_fingerprinted("")
                .await?
                .into_iter()
                .filter(|(key, _)| !seen_keys.contains(key))
                .collect();
            if batch.is_empty() {
                break;
            }
        }
        let scan = Scan {
            entries: found,
            problems,
        };
        self.absorb(&scan.entries);
        *self.problems.write().expect("problems lock") = scan.problems.clone();
        Ok(scan)
    }

    /// A file whose fingerprint still matches is taken from the cache without being read;
    /// every other file is read, digested and parsed.
    async fn parse_all(
        &self,
        listed: Vec<(String, Option<u64>)>,
    ) -> Result<(
        Vec<(String, u64, StdResult<Arc<MarkdownFile>, String>)>,
        bool,
    )> {
        let cached: Vec<Option<(u64, Arc<MarkdownFile>)>> = {
            let cache = self.parsed.read().expect("parse cache lock");
            listed
                .iter()
                .map(|(key, print)| {
                    let entry = cache.get(key)?;
                    (print.is_some() && entry.print == *print)
                        .then(|| (entry.digest, Arc::clone(&entry.file)))
                })
                .collect()
        };
        let to_read: Vec<String> = listed
            .iter()
            .zip(&cached)
            .filter(|(_, hit)| hit.is_none())
            .map(|((key, _), _)| key.clone())
            .collect();
        let contents = self.blobs.get_many(&to_read).await?;
        let digests: Vec<Option<u64>> = contents.iter().map(|b| b.as_deref().map(digest)).collect();
        let bytes: Vec<&[u8]> = contents.iter().flatten().map(Vec::as_slice).collect();
        let mut parsed = decode_in_parallel(&bytes).into_iter();
        let mut read = contents.iter().zip(digests);

        let mut cache = self.parsed.write().expect("parse cache lock");
        let mut decoded = Vec::with_capacity(listed.len());
        let mut vanished = false;
        for ((key, print), hit) in listed.into_iter().zip(cached) {
            if let Some((seen, file)) = hit {
                decoded.push((key, seen, Ok(file)));
                continue;
            }
            let (Some(_), Some(seen)) = read.next().expect("one read per miss") else {
                vanished = true;
                continue;
            };
            let result = parsed
                .next()
                .expect("one parse per file read")
                .map(Arc::new);
            if let Ok(file) = &result {
                cache.insert(
                    key.clone(),
                    Parsed {
                        print,
                        digest: seen,
                        file: Arc::clone(file),
                    },
                );
            }
            decoded.push((key, seen, result));
        }
        Ok((decoded, vanished))
    }

    fn absorb(&self, scanned: &[(Id, Located, Arc<MarkdownFile>)]) {
        let mut index = self.index.write().expect("index lock");
        let mut refreshed: HashMap<Id, Located> = HashMap::new();
        for (id, located, _) in scanned {
            let mut located = located.clone();
            if let Some(previous) = index.get(id) {
                located.seen = previous.seen;
            }
            refreshed.insert(id.clone(), located);
        }
        for (id, located) in index.iter() {
            refreshed
                .entry(id.clone())
                .or_insert_with(|| located.clone());
        }
        *index = refreshed;
    }

    pub fn locate(&self, id: &Id) -> Option<Located> {
        self.index.read().expect("index lock").get(id).cloned()
    }

    pub fn ids_of(&self, kind: EntityKind) -> Vec<Id> {
        self.index
            .read()
            .expect("index lock")
            .iter()
            .filter(|(_, located)| located.kind == kind)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub async fn read(&self, key: &str) -> Result<Option<MarkdownFile>> {
        let Some(bytes) = self.blobs.get(key).await? else {
            return Ok(None);
        };
        parse(&bytes, key).map(Some)
    }

    pub async fn read_by_id(&self, id: &Id) -> Result<Option<(String, MarkdownFile)>> {
        let Some((located, bytes)) = self.bytes_of(id).await? else {
            return Ok(None);
        };
        self.index.write().expect("index lock").insert(
            id.clone(),
            Located {
                seen: digest(&bytes),
                ..located.clone()
            },
        );
        let file = parse(&bytes, &located.key)?;
        Ok(Some((located.key, file)))
    }

    pub async fn peek_by_id(&self, id: &Id) -> Result<Option<(String, MarkdownFile)>> {
        let Some((located, bytes)) = self.bytes_of(id).await? else {
            return Ok(None);
        };
        let file = parse(&bytes, &located.key)?;
        Ok(Some((located.key, file)))
    }

    async fn bytes_of(&self, id: &Id) -> Result<Option<(Located, Vec<u8>)>> {
        for attempt in 0..LOOKUP_ATTEMPTS {
            let Some(located) = self.locate(id) else {
                return Ok(None);
            };
            if let Some(bytes) = self.blobs.get(&located.key).await? {
                return Ok(Some((located, bytes)));
            }
            if attempt > 0 {
                sleep(Duration::from_millis(2 * u64::from(attempt))).await;
            }
            self.reindex().await?;
        }
        self.index.write().expect("index lock").remove(id);
        Ok(None)
    }

    pub async fn write(
        &self,
        key: &str,
        file: &MarkdownFile,
        id: &Id,
        kind: EntityKind,
    ) -> Result<()> {
        self.write_if(key, file, id, kind, Precondition::Any).await
    }

    pub async fn write_if(
        &self,
        key: &str,
        file: &MarkdownFile,
        id: &Id,
        kind: EntityKind,
        precondition: Precondition,
    ) -> Result<()> {
        let previous = self.locate(id);
        let on_disk = match &previous {
            Some(located) => self.blobs.get(&located.key).await?,
            None => None,
        };
        let rendered = match on_disk.as_deref().map(str::from_utf8) {
            Some(Ok(original)) => file.render_over(original)?,
            _ => file.render()?,
        };

        match (precondition, &previous) {
            (Precondition::Unchanged, Some(located)) => {
                self.take(located, key, &rendered, id).await?;
            }
            (Precondition::Unchanged, None) => {
                if !self
                    .blobs
                    .compare_and_put(key, None, rendered.as_bytes())
                    .await?
                {
                    return Err(DomainError::conflict(format!(
                        "`{id}` already exists at `{key}`; reload it and reapply the change"
                    )));
                }
            }
            _ => {
                if let Some(located) = &previous
                    && located.key != key
                {
                    self.blobs.delete(&located.key).await?;
                }
                self.blobs.put(key, rendered.as_bytes()).await?;
            }
        }

        self.index.write().expect("index lock").insert(
            id.clone(),
            Located {
                key: key.to_owned(),
                kind,
                seen: digest(rendered.as_bytes()),
            },
        );
        Ok(())
    }

    async fn take(&self, located: &Located, key: &str, rendered: &str, id: &Id) -> Result<()> {
        let taken = self
            .blobs
            .compare_and_put(&located.key, Some(located.seen), rendered.as_bytes())
            .await?;
        if !taken {
            return Err(if self.blobs.exists(&located.key).await? {
                DomainError::conflict(format!(
                    "`{id}` changed since it was read; reload it and reapply the change"
                ))
            } else {
                DomainError::conflict(format!("`{id}` was deleted since it was read"))
            });
        }
        if located.key != key {
            self.blobs.put(key, rendered.as_bytes()).await?;
            self.blobs.delete(&located.key).await?;
        }
        Ok(())
    }

    /// Edits a list of entity refs in one frontmatter field, retrying when another writer got
    /// there first.
    pub async fn amend_refs(
        &self,
        id: &Id,
        kind: EntityKind,
        field: &str,
        edit: impl Fn(&mut Vec<String>),
    ) -> Result<Amended> {
        for attempt in 0..AMEND_ATTEMPTS {
            let Some((key, mut file)) = self.read_by_id(id).await? else {
                return Ok(Amended::Missing);
            };
            let mut targets = refs_in(file.frontmatter.get(field).unwrap_or(&Value::Null));
            let before = targets.clone();
            edit(&mut targets);
            if targets == before {
                return Ok(Amended::Unchanged);
            }
            if targets.is_empty() {
                file.frontmatter.remove(field);
            } else {
                targets.sort();
                file.frontmatter.set(
                    field,
                    Value::Array(targets.into_iter().map(Value::String).collect()),
                );
            }
            match self
                .write_if(&key, &file, id, kind, Precondition::Unchanged)
                .await
            {
                Err(DomainError::Conflict(_)) if attempt + 1 < AMEND_ATTEMPTS => {
                    sleep(backoff(attempt)).await;
                }
                other => return other.map(|()| Amended::Changed),
            }
        }
        unreachable!("the loop returns on its last attempt")
    }

    pub async fn relocate(&self, id: &Id, to: &str) -> Result<()> {
        let Some((key, file)) = self.read_by_id(id).await? else {
            return Err(DomainError::not_found("entity", id));
        };
        if key == to {
            return Ok(());
        }
        let kind = self.locate(id).map_or(EntityKind::Document, |l| l.kind);
        self.write_if(to, &file, id, kind, Precondition::Unchanged)
            .await
    }

    pub async fn remove(&self, id: &Id) -> Result<()> {
        let Some(located) = self.locate(id) else {
            return Ok(());
        };
        self.blobs.delete(&located.key).await?;
        self.index.write().expect("index lock").remove(id);
        Ok(())
    }

    pub async fn load_all(&self, kind: EntityKind) -> Result<Vec<(String, Arc<MarkdownFile>)>> {
        let scanned = self.scan().await?;

        let mut loaded: Vec<(String, Arc<MarkdownFile>)> = scanned
            .entries
            .into_iter()
            .filter(|(_, located, _)| located.kind == kind)
            .map(|(_, located, file)| (located.key, file))
            .collect();
        loaded.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(loaded)
    }
}

fn parse(bytes: &[u8], key: &str) -> Result<MarkdownFile> {
    decode(bytes).map_err(|detail| DomainError::validation(format!("{key}: {detail}")))
}

fn decode_in_parallel(files: &[&[u8]]) -> Vec<StdResult<MarkdownFile, String>> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    let chunk = files.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for part in files.chunks(chunk) {
            workers.push(scope.spawn(move || part.iter().map(|b| decode(b)).collect::<Vec<_>>()));
        }
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a parse worker panicked"))
            .collect()
    })
}

fn decode(bytes: &[u8]) -> StdResult<MarkdownFile, String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err("not valid UTF-8".to_owned());
    };
    if has_conflict_markers(text) {
        return Err("holds unresolved merge conflict markers".to_owned());
    }
    MarkdownFile::parse(text).map_err(|e| e.to_string())
}

fn has_conflict_markers(text: &str) -> bool {
    text.lines()
        .any(|line| line.starts_with("<<<<<<< ") || line.starts_with(">>>>>>> "))
}

fn backoff(attempt: u32) -> Duration {
    let jitter = u64::from(std::process::id() % 5);
    Duration::from_millis(u64::from(attempt) * 2 + jitter + 1)
}

pub(crate) fn refs_in(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn kind_from(declared: Option<&str>) -> EntityKind {
    match declared {
        Some("task") => EntityKind::Task,
        Some("message") => EntityKind::Message,
        Some("skill") => EntityKind::Skill,
        Some("agent") => EntityKind::Actor,
        _ => EntityKind::Document,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::MemoryBlobStore;
    use orchy_core::Frontmatter;
    use serde_json::json;

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    fn file(id: &str, kind: &str) -> MarkdownFile {
        let mut frontmatter = Frontmatter::new();
        frontmatter.set("id", json!(id));
        frontmatter.set("type", json!(kind));
        MarkdownFile {
            frontmatter,
            body: orchy_core::Body::new("body"),
        }
    }

    async fn vault() -> (Vault, Arc<MemoryBlobStore>) {
        let blobs = Arc::new(MemoryBlobStore::new());
        let vault = Vault::open(Arc::clone(&blobs) as Arc<dyn BlobStore>)
            .await
            .unwrap();
        (vault, blobs)
    }

    #[tokio::test]
    async fn an_entity_is_found_by_id_regardless_of_where_it_sits() {
        let (vault, _) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write(
                "anywhere/at/all.md",
                &file(A, "decision"),
                &id,
                EntityKind::Document,
            )
            .await
            .unwrap();

        let (key, found) = vault.read_by_id(&id).await.unwrap().unwrap();
        assert_eq!(key, "anywhere/at/all.md");
        assert_eq!(codec::id_of(&found), Some(A));
    }

    #[tokio::test]
    async fn moving_a_file_does_not_leave_the_old_copy_behind() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("tasks/open/x.md", &file(A, "task"), &id, EntityKind::Task)
            .await
            .unwrap();
        vault
            .write("tasks/done/x.md", &file(A, "task"), &id, EntityKind::Task)
            .await
            .unwrap();

        assert!(blobs.get("tasks/open/x.md").await.unwrap().is_none());
        assert!(blobs.get("tasks/done/x.md").await.unwrap().is_some());
        assert_eq!(vault.locate(&id).unwrap().key, "tasks/done/x.md");
    }

    #[tokio::test]
    async fn the_index_survives_a_reopen_by_rescanning() {
        let blobs = Arc::new(MemoryBlobStore::new());
        {
            let vault = Vault::open(Arc::clone(&blobs) as Arc<dyn BlobStore>)
                .await
                .unwrap();
            vault
                .write(
                    "notes/a.md",
                    &file(A, "note"),
                    &Id::new(A).unwrap(),
                    EntityKind::Document,
                )
                .await
                .unwrap();
        }
        let reopened = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        assert!(reopened.locate(&Id::new(A).unwrap()).is_some());
    }

    #[tokio::test]
    async fn entities_are_grouped_by_the_kind_declared_in_frontmatter() {
        let (vault, _) = vault().await;
        vault
            .write(
                "tasks/open/a.md",
                &file(A, "task"),
                &Id::new(A).unwrap(),
                EntityKind::Task,
            )
            .await
            .unwrap();
        vault
            .write(
                "notes/b.md",
                &file(B, "note"),
                &Id::new(B).unwrap(),
                EntityKind::Document,
            )
            .await
            .unwrap();

        assert_eq!(vault.ids_of(EntityKind::Task), vec![Id::new(A).unwrap()]);
        assert_eq!(
            vault.ids_of(EntityKind::Document),
            vec![Id::new(B).unwrap()]
        );
    }

    #[tokio::test]
    async fn a_file_without_an_id_is_skipped_rather_than_failing_the_scan() {
        let blobs = Arc::new(MemoryBlobStore::new());
        blobs.put("README.md", b"# Just prose\n").await.unwrap();
        blobs
            .put("half.md", b"---\ntype: note\n---\n\nno id\n")
            .await
            .unwrap();

        let vault = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        assert!(vault.ids_of(EntityKind::Document).is_empty());
    }

    #[tokio::test]
    async fn runtime_files_are_never_indexed() {
        let blobs = Arc::new(MemoryBlobStore::new());
        blobs
            .put(
                ".orchy/scratch.md",
                format!("---\nid: {A}\ntype: note\n---\n\nx\n").as_bytes(),
            )
            .await
            .unwrap();
        let vault = Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap();
        assert!(vault.locate(&Id::new(A).unwrap()).is_none());
    }

    #[tokio::test]
    async fn a_write_is_refused_when_the_file_moved_on_since_it_was_read() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();
        vault.read_by_id(&id).await.unwrap();

        blobs
            .put("notes/a.md", b"---\nid: x\n---\n\nsomeone else\n")
            .await
            .unwrap();

        let refused = vault
            .write_if(
                "notes/a.md",
                &file(A, "note"),
                &id,
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "the later writer is told, not quietly preferred: {refused:?}"
        );
    }

    #[tokio::test]
    async fn a_peek_leaves_the_precondition_where_the_last_real_read_put_it() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();
        vault.read_by_id(&id).await.unwrap();

        blobs
            .put("notes/a.md", b"---\nid: x\n---\n\nsomeone else\n")
            .await
            .unwrap();
        vault.peek_by_id(&id).await.unwrap();

        let refused = vault
            .write_if(
                "notes/a.md",
                &file(A, "note"),
                &id,
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "a peek taken on the way to a write must not vouch for the bytes: {refused:?}"
        );
    }

    #[tokio::test]
    async fn an_entity_this_process_never_saw_must_not_already_be_on_disk() {
        let (vault, blobs) = vault().await;
        blobs.put("notes/a.md", b"already here").await.unwrap();

        let refused = vault
            .write_if(
                "notes/a.md",
                &file(A, "note"),
                &Id::new(A).unwrap(),
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await;
        assert!(
            matches!(refused, Err(DomainError::Conflict(_))),
            "creating claims the key rather than overwriting it: {refused:?}"
        );
    }

    #[tokio::test]
    async fn a_rescan_relocates_an_entity_without_forgetting_what_was_read_of_it() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("tasks/open/x.md", &file(A, "task"), &id, EntityKind::Task)
            .await
            .unwrap();
        vault.read_by_id(&id).await.unwrap();
        let seen = vault.locate(&id).unwrap().seen;

        let bytes = blobs.get("tasks/open/x.md").await.unwrap().unwrap();
        blobs.put("tasks/done/x.md", &bytes).await.unwrap();
        blobs.delete("tasks/open/x.md").await.unwrap();

        let (key, _) = vault.read_by_id(&id).await.unwrap().unwrap();
        assert_eq!(
            key, "tasks/done/x.md",
            "refiled by somebody else, still found"
        );
        assert_ne!(seen, 0);
    }

    #[tokio::test]
    async fn an_entity_deleted_by_somebody_else_is_reported_gone() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();

        blobs.delete("notes/a.md").await.unwrap();
        assert!(vault.read_by_id(&id).await.unwrap().is_none());
        assert!(vault.locate(&id).is_none(), "and dropped from the index");
    }

    #[tokio::test]
    async fn removing_clears_both_the_file_and_the_index() {
        let (vault, blobs) = vault().await;
        let id = Id::new(A).unwrap();
        vault
            .write("notes/a.md", &file(A, "note"), &id, EntityKind::Document)
            .await
            .unwrap();
        vault.remove(&id).await.unwrap();

        assert!(vault.locate(&id).is_none());
        assert!(blobs.get("notes/a.md").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_rescan_sees_a_file_rewritten_on_disk_even_at_the_same_size() {
        use crate::blob::FsBlobStore;

        let temp = tempfile::tempdir().unwrap();
        let blobs = Arc::new(FsBlobStore::new(temp.path()));
        let note = |title: &str| format!("---\nid: {A}\ntype: note\ntitle: {title}\n---\n");
        blobs
            .put("docs/a.md", note("first").as_bytes())
            .await
            .unwrap();
        let vault = Vault::open(Arc::clone(&blobs) as Arc<dyn BlobStore>)
            .await
            .unwrap();

        std::fs::write(temp.path().join("docs/a.md"), note("again")).unwrap();
        let loaded = vault.load_all(EntityKind::Document).await.unwrap();
        assert_eq!(
            loaded[0].1.frontmatter.string("title"),
            Some("again"),
            "an in-place edit is seen"
        );

        blobs
            .put("docs/a.md", note("third").as_bytes())
            .await
            .unwrap();
        let loaded = vault.load_all(EntityKind::Document).await.unwrap();
        assert_eq!(
            loaded[0].1.frontmatter.string("title"),
            Some("third"),
            "a replaced file is seen"
        );
    }
}
