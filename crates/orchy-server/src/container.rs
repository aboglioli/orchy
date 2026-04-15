use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use orchy_core::agent::AgentId;
use orchy_core::agent::service::AgentService;
use orchy_core::knowledge::service::KnowledgeService;
use orchy_core::message::service::MessageService;
use orchy_core::project::service::ProjectService;
use orchy_core::project_link::service::ProjectLinkService;
use orchy_core::resource_lock::service::LockService;
use orchy_core::task::service::TaskService;
use orchy_store_memory::MemoryBackend;
use orchy_store_pg::PgBackend;
use orchy_store_sqlite::SqliteBackend;

use crate::config::{Config, EmbeddingsConfig};
use crate::embeddings::{EmbeddingsBackend, OpenAiEmbeddingsProvider};
use crate::store::StoreBackend;

pub struct Container {
    pub store: Arc<StoreBackend>,
    pub task_service: TaskService<StoreBackend, StoreBackend>,
    pub agent_service: AgentService<StoreBackend>,
    pub message_service: MessageService<StoreBackend, StoreBackend>,
    pub project_service: ProjectService<StoreBackend>,
    pub project_link_service: ProjectLinkService<StoreBackend>,
    pub knowledge_service: KnowledgeService<StoreBackend, EmbeddingsBackend>,
    pub lock_service: LockService<StoreBackend>,
    pub session_agents: Arc<RwLock<HashMap<String, AgentId>>>,
    pub config: Config,
}

impl Container {
    pub async fn from_config(config: Config) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let store = Arc::new(Self::build_store(&config).await?);
        let embeddings: Option<Arc<EmbeddingsBackend>> = config
            .embeddings
            .as_ref()
            .map(Self::build_embeddings)
            .transpose()?
            .map(Arc::new);

        let task_service = TaskService::new(Arc::clone(&store), Arc::clone(&store));
        let agent_service = AgentService::new(Arc::clone(&store));
        let message_service = MessageService::new(Arc::clone(&store), Arc::clone(&store));
        let knowledge_service = KnowledgeService::new(Arc::clone(&store), embeddings);
        let project_service = ProjectService::new(Arc::clone(&store));
        let project_link_service = ProjectLinkService::new(Arc::clone(&store));
        let lock_service = LockService::new(Arc::clone(&store));

        Ok(Arc::new(Self {
            store,
            task_service,
            agent_service,
            message_service,
            project_service,
            knowledge_service,
            project_link_service,
            lock_service,
            session_agents: Arc::new(RwLock::new(HashMap::new())),
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
                let store_config = config
                    .store
                    .sqlite
                    .as_ref()
                    .ok_or("store.sqlite config required when backend = \"sqlite\"")?;
                let backend = SqliteBackend::new(&store_config.path, embedding_dims)?;
                backend.run_migrations(std::path::Path::new("migrations/sqlite"))?;
                Ok(StoreBackend::Sqlite(backend))
            }
            "postgres" => {
                let store_config = config
                    .store
                    .postgres
                    .as_ref()
                    .ok_or("store.postgres config required when backend = \"postgres\"")?;
                let backend = PgBackend::new(&store_config.url, embedding_dims).await?;
                backend
                    .run_migrations(std::path::Path::new("migrations/postgres"))
                    .await?;
                Ok(StoreBackend::Postgres(backend))
            }
            other => Err(format!("unsupported store backend: {other}").into()),
        }
    }

    fn build_embeddings(
        config: &EmbeddingsConfig,
    ) -> Result<EmbeddingsBackend, Box<dyn std::error::Error>> {
        match config.provider.as_str() {
            "openai" => {
                let openai = config
                    .openai
                    .as_ref()
                    .ok_or("embeddings.openai config required when provider = \"openai\"")?;
                Ok(EmbeddingsBackend::OpenAi(OpenAiEmbeddingsProvider::new(
                    openai.url.clone(),
                    openai.model.clone(),
                    openai.dimensions,
                )))
            }
            other => Err(format!("unsupported embeddings provider: {other}").into()),
        }
    }
}
