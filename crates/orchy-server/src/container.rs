use std::sync::Arc;

use orchy_core::agent::service::AgentService;
use orchy_core::embeddings::{EmbeddingsBackend, OpenAiEmbeddingsProvider};
use orchy_core::memory::service::{ContextService, MemoryService};
use orchy_core::message::service::MessageService;
use orchy_core::skill::service::SkillService;
use orchy_core::task::service::TaskService;
use orchy_store_memory::MemoryBackend;
use orchy_store_pg::PgBackend;
use orchy_store_sqlite::SqliteBackend;

use crate::config::{Config, EmbeddingsConfig};
use crate::store::StoreBackend;

pub struct Container {
    pub task_service: TaskService<StoreBackend>,
    pub memory_service: MemoryService<StoreBackend>,
    pub agent_service: AgentService<StoreBackend>,
    pub message_service: MessageService<StoreBackend>,
    pub context_service: ContextService<StoreBackend>,
    pub skill_service: SkillService<StoreBackend>,
    pub config: Config,
}

impl Container {
    pub async fn from_config(config: Config) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let store = Arc::new(Self::build_store(&config).await?);
        let embeddings = config
            .embeddings
            .as_ref()
            .map(|e| Arc::new(Self::build_embeddings(e)));

        let task_service = TaskService::new(Arc::clone(&store));
        let memory_service = MemoryService::new(Arc::clone(&store), embeddings.clone());
        let agent_service = AgentService::new(Arc::clone(&store));
        let message_service = MessageService::new(Arc::clone(&store));
        let context_service = ContextService::new(Arc::clone(&store), embeddings);
        let skill_service = SkillService::new(Arc::clone(&store));

        Ok(Arc::new(Self {
            task_service,
            memory_service,
            agent_service,
            message_service,
            context_service,
            skill_service,
            config,
        }))
    }

    async fn build_store(config: &Config) -> Result<StoreBackend, Box<dyn std::error::Error>> {
        let embedding_dims = config
            .embeddings
            .as_ref()
            .and_then(|e| e.openai.as_ref())
            .map(|o| o.dimensions);

        match config.store.backend.as_str() {
            "memory" => Ok(StoreBackend::Memory(MemoryBackend::new())),
            "sqlite" => {
                let path = &config
                    .store
                    .sqlite
                    .as_ref()
                    .expect("store.sqlite config required when backend = \"sqlite\"")
                    .path;
                Ok(StoreBackend::Sqlite(SqliteBackend::new(
                    path,
                    embedding_dims,
                )?))
            }
            "postgres" => {
                let url = &config
                    .store
                    .postgres
                    .as_ref()
                    .expect("store.postgres config required when backend = \"postgres\"")
                    .url;
                Ok(StoreBackend::Postgres(
                    PgBackend::new(url, embedding_dims).await?,
                ))
            }
            other => Err(format!("unsupported store backend: {other}").into()),
        }
    }

    fn build_embeddings(config: &EmbeddingsConfig) -> EmbeddingsBackend {
        match config.provider.as_str() {
            "openai" => {
                let openai = config
                    .openai
                    .as_ref()
                    .expect("embeddings.openai config required when provider = \"openai\"");
                EmbeddingsBackend::OpenAi(OpenAiEmbeddingsProvider::new(
                    openai.url.clone(),
                    openai.model.clone(),
                    openai.dimensions,
                ))
            }
            other => panic!("unsupported embeddings provider: {other}"),
        }
    }
}
