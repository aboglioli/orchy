use std::sync::Arc;

use orchy_core::agent::service::AgentService;
use orchy_core::memory::service::{ContextService, MemoryService};
use orchy_core::message::service::MessageService;
use orchy_core::project::service::ProjectService;
use orchy_core::project_link::service::ProjectLinkService;
use orchy_core::resource_lock::service::LockService;
use orchy_core::skill::service::SkillService;
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
    pub memory_service: MemoryService<StoreBackend, EmbeddingsBackend>,
    pub agent_service: AgentService<StoreBackend>,
    pub message_service: MessageService<StoreBackend, StoreBackend>,
    pub context_service: ContextService<StoreBackend, EmbeddingsBackend>,
    pub skill_service: SkillService<StoreBackend>,
    pub project_service: ProjectService<StoreBackend>,
    pub project_link_service: ProjectLinkService<StoreBackend>,
    pub lock_service: LockService<StoreBackend>,
    pub config: Config,
}

impl Container {
    pub async fn from_config(config: Config) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let store = Arc::new(Self::build_store(&config).await?);
        let embeddings: Option<Arc<EmbeddingsBackend>> = config
            .embeddings
            .as_ref()
            .map(|e| Arc::new(Self::build_embeddings(e)));

        let task_service = TaskService::new(Arc::clone(&store), Arc::clone(&store));
        let memory_service = MemoryService::new(Arc::clone(&store), embeddings.clone());
        let agent_service = AgentService::new(Arc::clone(&store));
        let message_service = MessageService::new(Arc::clone(&store), Arc::clone(&store));
        let context_service = ContextService::new(Arc::clone(&store), embeddings);
        let skill_service = SkillService::new(Arc::clone(&store));
        let project_service = ProjectService::new(Arc::clone(&store));
        let project_link_service = ProjectLinkService::new(Arc::clone(&store));
        let lock_service = LockService::new(Arc::clone(&store));

        Ok(Arc::new(Self {
            store,
            task_service,
            memory_service,
            agent_service,
            message_service,
            context_service,
            skill_service,
            project_service,
            project_link_service,
            lock_service,
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
                    .expect("store.sqlite config required when backend = \"sqlite\"");
                let backend = SqliteBackend::new(&store_config.path, embedding_dims)?;
                backend.run_migrations(std::path::Path::new("migrations/sqlite"))?;
                Ok(StoreBackend::Sqlite(backend))
            }
            "postgres" => {
                let store_config = config
                    .store
                    .postgres
                    .as_ref()
                    .expect("store.postgres config required when backend = \"postgres\"");
                let backend = PgBackend::new(&store_config.url, embedding_dims).await?;
                backend
                    .run_migrations(std::path::Path::new("migrations/postgres"))
                    .await?;
                Ok(StoreBackend::Postgres(backend))
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
