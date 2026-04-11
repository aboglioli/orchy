pub mod agent_store;
pub mod context_store;
pub mod memory_store;
pub mod message_store;
pub mod task_store;

pub use agent_store::AgentStore;
pub use context_store::ContextStore;
pub use memory_store::MemoryStore;
pub use message_store::MessageStore;
pub use task_store::TaskStore;

/// Enum dispatch for storage backends. Resolved once at startup.
/// Variants and delegation methods added as backends are implemented.
pub enum Store {
    // Memory(orchy_store_memory::MemoryBackend),
    // Sqlite(orchy_store_sqlite::SqliteBackend),
    // Postgres(orchy_store_pg::PgBackend),
}
