use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use fs4::fs_std::FileExt;
use orchy_core::{
    Actor, ActorId, ActorStore, Clock, DomainError, Lease, LeaseStore, ResourceKey, Result,
};

use crate::codec;
use crate::vault::Vault;

const PRESENCE_TTL_SECS: i64 = 300;

pub struct VaultActorStore {
    vault: Arc<Vault>,
}

impl VaultActorStore {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl ActorStore for VaultActorStore {
    async fn get(&self, id: &ActorId) -> Result<Option<Actor>> {
        let key = self.vault.layout().actor_key(id);
        let Some(file) = self.vault.read(&key).await? else {
            return Ok(None);
        };
        codec::actor_from_markdown(&file, &key).map(Some)
    }

    async fn roster(&self) -> Result<Vec<Actor>> {
        let mut actors = Vec::new();
        for key in self.vault.blobs().list(crate::layout::AGENTS).await? {
            if !self.vault.layout().is_markdown(&key) {
                continue;
            }
            if let Some(file) = self.vault.read(&key).await? {
                actors.push(codec::actor_from_markdown(&file, &key)?);
            }
        }
        actors.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(actors)
    }

    async fn save(&self, actor: &mut Actor) -> Result<()> {
        let key = self.vault.layout().actor_key(actor.id());
        let carried = match self.vault.read(&key).await? {
            Some(file) => codec::carried_frontmatter(&file),
            None => Default::default(),
        };
        let file = codec::actor_to_markdown(actor, carried);
        let bytes = file.render()?;
        self.vault.blobs().put(&key, bytes.as_bytes()).await?;

        let presence = self.vault.layout().presence_key(actor.id());
        let stamp = serde_json::json!({
            "actor": actor.id().to_string(),
            "last_seen": actor.last_seen().to_rfc3339(),
        });
        self.vault
            .blobs()
            .put(&presence, stamp.to_string().as_bytes())
            .await
    }

    async fn present(&self, now: DateTime<Utc>) -> Result<Vec<ActorId>> {
        let cutoff = now - Duration::seconds(PRESENCE_TTL_SECS);
        let mut present = Vec::new();
        for key in self
            .vault
            .blobs()
            .list(&format!("{}/presence", crate::layout::RUNTIME))
            .await?
        {
            let Some(bytes) = self.vault.blobs().get(&key).await? else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let (Some(actor), Some(seen)) = (
                value.get("actor").and_then(|v| v.as_str()),
                value.get("last_seen").and_then(|v| v.as_str()),
            ) else {
                continue;
            };
            let Ok(seen) = DateTime::parse_from_rfc3339(seen) else {
                continue;
            };
            if seen.with_timezone(&Utc) >= cutoff
                && let Ok(id) = actor.parse::<ActorId>()
            {
                present.push(id);
            }
        }
        present.sort();
        Ok(present)
    }

    async fn touch(&self, id: &ActorId, now: DateTime<Utc>) -> Result<()> {
        let mut actor = self
            .get(id)
            .await?
            .ok_or_else(|| DomainError::not_found("actor", id))?;
        actor.seen_at(now);
        self.save(&mut actor).await
    }
}

/// Mutual exclusion on this machine: an advisory `flock` for the duration of the acquire, and
/// a TTL record so a holder that dies does not block the resource forever. Both are needed —
/// the lock orders concurrent acquires, the record survives the process.
pub struct FileLeaseStore {
    root: PathBuf,
    clock: Arc<dyn Clock>,
}

impl FileLeaseStore {
    pub fn new(root: impl Into<PathBuf>, clock: Arc<dyn Clock>) -> Self {
        Self {
            root: root.into(),
            clock,
        }
    }

    fn lock_path(&self, key: &ResourceKey) -> PathBuf {
        let safe: String = key
            .as_str()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        self.root.join(format!("{safe}.lock"))
    }

    fn read_record(&self, key: &ResourceKey) -> Option<LeaseRecord> {
        let bytes = std::fs::read(self.lock_path(key)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LeaseRecord {
    holder: String,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    generation: u64,
}

#[async_trait]
impl LeaseStore for FileLeaseStore {
    async fn acquire(&self, key: &ResourceKey, by: &ActorId, ttl: Duration) -> Result<Lease> {
        std::fs::create_dir_all(&self.root)
            .map_err(|e| DomainError::validation(format!("creating lock directory: {e}")))?;
        let path = self.lock_path(key);
        let now = self.clock.now();

        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| DomainError::validation(format!("opening lock: {e}")))?;
        FileExt::lock_exclusive(&file)
            .map_err(|e| DomainError::validation(format!("locking: {e}")))?;

        let held = self.read_record(key);
        if let Some(record) = &held
            && record.expires_at > now
            && record.holder != by.to_string()
        {
            let _ = FileExt::unlock(&file);
            return Err(DomainError::conflict(format!(
                "`{key}` is held by {} until {}",
                record.holder, record.expires_at
            )));
        }

        let generation = held.as_ref().map_or(0, |r| r.generation) + 1;
        let record = LeaseRecord {
            holder: by.to_string(),
            acquired_at: now,
            expires_at: now + ttl,
            generation,
        };
        std::fs::write(&path, serde_json::to_vec(&record).unwrap_or_default())
            .map_err(|e| DomainError::validation(format!("writing lock: {e}")))?;
        let _ = FileExt::unlock(&file);

        Ok(Lease::new(
            key.clone(),
            by.clone(),
            record.acquired_at,
            record.expires_at,
            generation,
        ))
    }

    async fn release(&self, key: &ResourceKey, by: &ActorId) -> Result<()> {
        let Some(record) = self.read_record(key) else {
            return Ok(());
        };
        if record.holder != by.to_string() && record.expires_at > self.clock.now() {
            return Err(DomainError::forbidden(format!(
                "`{key}` is held by {}, not {by}",
                record.holder
            )));
        }
        let _ = std::fs::remove_file(self.lock_path(key));
        Ok(())
    }

    async fn check(&self, key: &ResourceKey) -> Result<Option<Lease>> {
        let Some(record) = self.read_record(key) else {
            return Ok(None);
        };
        if record.expires_at <= self.clock.now() {
            return Ok(None);
        }
        let Ok(holder) = record.holder.parse::<ActorId>() else {
            return Ok(None);
        };
        Ok(Some(Lease::new(
            key.clone(),
            holder,
            record.acquired_at,
            record.expires_at,
            record.generation,
        )))
    }
}
