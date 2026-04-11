use crate::entities::{MemoryEntry, MemoryFilter, WriteMemory};
use crate::error::Result;
use crate::value_objects::Namespace;

pub trait MemoryStore: Send + Sync {
    async fn write(&self, entry: WriteMemory) -> Result<MemoryEntry>;
    async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>>;
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>>;
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>>;
    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()>;
}
