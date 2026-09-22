use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use fs4::fs_std::FileExt;
use orchy_core::{ActorId, DomainError, Id, ReadWatermarks, Result};

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

    /// Locked rather than replaced, because one actor still runs several orchy processes at
    /// once: a rename would swap the inode out from under the lock, and a bare truncate lets a
    /// reader see the empty middle and conclude nothing has ever been read.
    fn open(&self, actor: &ActorId) -> Result<File> {
        std::fs::create_dir_all(&self.root)
            .map_err(|e| DomainError::validation(format!("creating watermark directory: {e}")))?;
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.path(actor))
            .map_err(|e| DomainError::validation(format!("opening watermark: {e}")))
    }
}

fn read_mark(file: &mut File) -> Option<Id> {
    let mut text = String::new();
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_to_string(&mut text).ok()?;
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
        let mut file = self.open(actor)?;
        FileExt::lock_shared(&file)
            .map_err(|e| DomainError::validation(format!("locking watermark: {e}")))?;
        let mark = read_mark(&mut file);
        let _ = FileExt::unlock(&file);
        Ok(mark)
    }

    /// Ids are time-ordered, so the mark only moves forward: otherwise one agent's two
    /// processes race and the older mark resurrects everything in between.
    fn advance(&self, actor: &ActorId, to: &Id) -> Result<()> {
        let mut file = self.open(actor)?;
        FileExt::lock_exclusive(&file)
            .map_err(|e| DomainError::validation(format!("locking watermark: {e}")))?;

        let written = (|| -> Result<()> {
            if read_mark(&mut file).is_some_and(|current| current >= *to) {
                return Ok(());
            }
            let value =
                serde_json::json!({ "actor": actor.to_string(), "watermark": to.to_string() });
            file.set_len(0)
                .and_then(|()| file.seek(SeekFrom::Start(0)))
                .and_then(|_| file.write_all(value.to_string().as_bytes()))
                .and_then(|()| file.sync_all())
                .map_err(|e| DomainError::validation(format!("writing watermark: {e}")))
        })();

        let _ = FileExt::unlock(&file);
        written
    }
}
