use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use orchy_core::{ActorId, DomainError, Id, ReadWatermarks, Result};

use crate::lock::{DEFAULT_WAIT, FileLock};

/// Per actor, per machine, never tracked: a broadcast to six agents would otherwise put six
/// writers on one file and record that agents looked at things.
pub struct FileWatermarks {
    root: PathBuf,
}

impl FileWatermarks {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, actor: &ActorId) -> PathBuf {
        self.root.join(format!("{actor}.json"))
    }
}

/// Locked rather than replaced, because one actor still runs several orchy processes at once:
/// a rename would swap the inode out from under the lock, and a bare truncate lets a reader
/// see the empty middle and conclude nothing has ever been read.
fn read_mark(file: &File) -> Option<Id> {
    let mut handle = file;
    let mut text = String::new();
    handle.seek(SeekFrom::Start(0)).ok()?;
    handle.read_to_string(&mut text).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("watermark")
        .and_then(|v| v.as_str())
        .and_then(|s| Id::new(s).ok())
}

impl ReadWatermarks for FileWatermarks {
    fn watermark(&self, actor: &ActorId) -> Result<Option<Id>> {
        if !self.path(actor).exists() {
            return Ok(None);
        }
        let lock = FileLock::shared(&self.path(actor), "watermark", DEFAULT_WAIT)?;
        Ok(read_mark(lock.file()))
    }

    /// Ids are time-ordered, so the mark only moves forward: otherwise one agent's two
    /// processes race and the older mark resurrects everything in between.
    fn advance(&self, actor: &ActorId, to: &Id) -> Result<()> {
        let lock = FileLock::exclusive(&self.path(actor), "watermark", DEFAULT_WAIT)?;
        let file = lock.file();
        if read_mark(file).is_some_and(|current| current >= *to) {
            return Ok(());
        }

        let mut handle = file;
        let value = serde_json::json!({ "actor": actor.to_string(), "watermark": to.to_string() });
        file.set_len(0)
            .and_then(|()| handle.seek(SeekFrom::Start(0)))
            .and_then(|_| handle.write_all(value.to_string().as_bytes()))
            .and_then(|()| file.sync_all())
            .map_err(|e| DomainError::validation(format!("writing watermark: {e}")))
    }
}
