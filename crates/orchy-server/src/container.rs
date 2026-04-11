use std::sync::Arc;

use orchy_core::embeddings::{EmbeddingsBackend, OpenAiEmbeddingsProvider};
use orchy_core::services::{
    AgentService, ContextService, MemoryService, MessageService, TaskService,
};
use orchy_store_memory::MemoryBackend;

use crate::config::{Config, EmbeddingsConfig};

pub struct Container {
    pub task_service: TaskService<MemoryBackend>,
    pub memory_service: MemoryService<MemoryBackend>,
    pub agent_service: AgentService<MemoryBackend>,
    pub message_service: MessageService<MemoryBackend>,
    pub context_service: ContextService<MemoryBackend>,
    pub config: Config,
}

impl Container {
    pub fn from_config(config: Config) -> Self {
        let store = Arc::new(Self::build_store(&config));
        let embeddings = config
            .embeddings
            .as_ref()
            .map(|e| Arc::new(Self::build_embeddings(e)));

        let task_service = TaskService::new(Arc::clone(&store));
        let memory_service = MemoryService::new(Arc::clone(&store), embeddings.clone());
        let agent_service = AgentService::new(Arc::clone(&store));
        let message_service = MessageService::new(Arc::clone(&store));
        let context_service = ContextService::new(Arc::clone(&store), embeddings);

        Self {
            task_service,
            memory_service,
            agent_service,
            message_service,
            context_service,
            config,
        }
    }

    fn build_store(config: &Config) -> MemoryBackend {
        match config.store.backend.as_str() {
            "memory" => MemoryBackend::new(),
            other => panic!("unsupported store backend: {other}"),
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
