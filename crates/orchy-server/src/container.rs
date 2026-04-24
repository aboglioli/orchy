use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use orchy_application::{Application, EventQuery};
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::graph::EdgeStore;
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::NamespaceStore;
use orchy_core::organization::OrganizationStore;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::{OrgMembershipStore, PasswordHasher, UserStore};
use orchy_store_memory::*;
use orchy_store_pg::PgDatabase;
use orchy_store_sqlite::SqliteDatabase;

use crate::auth::{BcryptPasswordHasher, JwtTokenEncoder};
use crate::config::{Config, EmbeddingsConfig};
use crate::embeddings::{EmbeddingsBackend, OpenAiEmbeddingsProvider};
use crate::event_query::{MemoryEventQueryAdapter, PgEventQueryAdapter, SqliteEventQueryAdapter};

pub struct Container {
    pub agent_store: Arc<dyn AgentStore>,
    pub namespace_store: Arc<dyn NamespaceStore>,
    pub edge_store: Arc<dyn EdgeStore>,
    pub orgs: Arc<dyn OrganizationStore>,
    pub memberships: Arc<dyn OrgMembershipStore>,
    pub app: Application,
    pub session_agents: Arc<RwLock<HashMap<String, AgentId>>>,
    pub config: Config,
    pub start_time: std::time::Instant,
    pub jwt_encoder: Option<JwtTokenEncoder>,
    pub password_hasher: Arc<dyn PasswordHasher>,
}

struct Stores {
    agents: Arc<dyn AgentStore>,
    tasks: Arc<dyn TaskStore>,
    projects: Arc<dyn ProjectStore>,
    knowledge: Arc<dyn KnowledgeStore>,
    messages: Arc<dyn MessageStore>,
    locks: Arc<dyn LockStore>,
    namespaces: Arc<dyn NamespaceStore>,
    orgs: Arc<dyn OrganizationStore>,
    edges: Arc<dyn EdgeStore>,
    event_query: Arc<dyn EventQuery>,
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl Container {
    pub async fn from_config(config: Config) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let stores = Self::build_stores(&config).await?;
        let embeddings: Option<Arc<EmbeddingsBackend>> = config
            .embeddings
            .as_ref()
            .map(Self::build_embeddings)
            .transpose()?
            .map(Arc::new);

        let app = Application::new(
            stores.agents.clone(),
            stores.tasks.clone(),
            stores.projects.clone(),
            stores.knowledge.clone(),
            stores.messages.clone(),
            stores.locks.clone(),
            stores.namespaces.clone(),
            stores.orgs.clone(),
            stores.edges.clone(),
            embeddings.map(|e| e as Arc<dyn EmbeddingsProvider>),
            stores.event_query.clone(),
            stores.users.clone(),
            stores.memberships.clone(),
        );

        let password_hasher: Arc<dyn PasswordHasher> =
            Arc::new(BcryptPasswordHasher::with_cost(config.auth.bcrypt_cost));

        let jwt_encoder = Self::init_jwt_encoder(&config).await?;

        let container = Arc::new(Self {
            agent_store: stores.agents,
            namespace_store: stores.namespaces,
            edge_store: stores.edges,
            orgs: stores.orgs,
            memberships: stores.memberships,
            app,
            session_agents: Arc::new(RwLock::new(HashMap::new())),
            config,
            start_time: std::time::Instant::now(),
            jwt_encoder,
            password_hasher: password_hasher.clone(),
        });

        container.bootstrap_admin(&password_hasher).await?;

        Ok(container)
    }

    async fn init_jwt_encoder(
        config: &Config,
    ) -> Result<Option<JwtTokenEncoder>, Box<dyn std::error::Error>> {
        let keys_dir = std::path::Path::new(&config.auth.keys_dir);
        let private_key_path = keys_dir.join("private.pem");
        let public_key_path = keys_dir.join("public.pem");

        let (private_pem, public_pem) = if private_key_path.exists() && public_key_path.exists() {
            (
                tokio::fs::read_to_string(&private_key_path).await?,
                tokio::fs::read_to_string(&public_key_path).await?,
            )
        } else {
            tracing::info!("Generating new RSA keypair for JWT signing");
            let (private_pem, public_pem) = crate::auth::generate_rsa_keypair()
                .map_err(|e| format!("failed to generate RSA keys: {e}"))?;

            tokio::fs::create_dir_all(keys_dir).await?;
            tokio::fs::write(&private_key_path, &private_pem).await?;
            tokio::fs::write(&public_key_path, &public_pem).await?;
            tracing::info!("RSA keypair saved to {}", keys_dir.display());

            (private_pem, public_pem)
        };

        let encoder = JwtTokenEncoder::from_rsa_pem(
            private_pem.as_bytes(),
            public_pem.as_bytes(),
            config.auth.jwt_duration_hours,
        )
        .map_err(|e| format!("failed to create JWT encoder: {e}"))?;

        Ok(Some(encoder))
    }

    async fn bootstrap_admin(
        &self,
        hasher: &Arc<dyn PasswordHasher>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.app.bootstrap_admin.execute(hasher.as_ref()).await {
            Ok(Some(user)) => {
                tracing::info!(
                    "Bootstrap admin user created: {} (id: {})",
                    user.email,
                    user.id
                );
                tracing::info!("Default credentials: admin@orchy.sh / 12345678");
            }
            Ok(None) => {
                tracing::debug!("Admin user already exists, skipping bootstrap");
            }
            Err(e) => {
                tracing::error!("Failed to bootstrap admin user: {e}");
            }
        }
        Ok(())
    }

    async fn build_stores(config: &Config) -> Result<Stores, Box<dyn std::error::Error>> {
        let embedding_dims = config
            .embeddings
            .as_ref()
            .and_then(|e| e.openai.as_ref())
            .map(|o| o.dimensions);

        match config.store.backend.as_str() {
            "memory" => Ok(Self::build_memory_stores()),
            "sqlite" => {
                let store_config = config
                    .store
                    .sqlite
                    .as_ref()
                    .ok_or("store.sqlite config required when backend = \"sqlite\"")?;
                let backend = SqliteDatabase::new(&store_config.path, embedding_dims)?;
                backend.run_migrations(std::path::Path::new("migrations/sqlite"))?;
                Ok(Self::build_sqlite_stores(backend))
            }
            "postgres" => {
                let store_config = config
                    .store
                    .postgres
                    .as_ref()
                    .ok_or("store.postgres config required when backend = \"postgres\"")?;
                let backend = PgDatabase::new(&store_config.url, embedding_dims).await?;
                backend
                    .run_migrations(std::path::Path::new("migrations/postgres"))
                    .await?;
                Ok(Self::build_pg_stores(backend))
            }
            other => Err(format!("unsupported store backend: {other}").into()),
        }
    }

    fn build_memory_stores() -> Stores {
        let state = Arc::new(MemoryState::new());
        Stores {
            agents: Arc::new(MemoryAgentStore::new(state.clone())),
            tasks: Arc::new(MemoryTaskStore::new(state.clone())),
            projects: Arc::new(MemoryProjectStore::new(state.clone())),
            knowledge: Arc::new(MemoryKnowledgeStore::new(state.clone())),
            messages: Arc::new(MemoryMessageStore::new(state.clone())),
            locks: Arc::new(MemoryLockStore::new(state.clone())),
            namespaces: Arc::new(MemoryNamespaceStore::new(state.clone())),
            orgs: Arc::new(MemoryOrganizationStore::new(state.clone())),
            edges: Arc::new(MemoryEdgeStore::new(state.clone())),
            event_query: Arc::new(MemoryEventQueryAdapter(MemoryEventQuery::new(
                state.clone(),
            ))),
            users: Arc::new(MemoryUserStore::new(state.clone())),
            memberships: Arc::new(MemoryOrgMembershipStore::new(state)),
        }
    }

    fn build_sqlite_stores(backend: SqliteDatabase) -> Stores {
        use orchy_store_sqlite::*;
        let conn = backend.conn();
        Stores {
            agents: Arc::new(SqliteAgentStore::new(conn.clone())),
            tasks: Arc::new(SqliteTaskStore::new(conn.clone())),
            projects: Arc::new(SqliteProjectStore::new(conn.clone())),
            knowledge: Arc::new(SqliteKnowledgeStore::new(conn.clone())),
            messages: Arc::new(SqliteMessageStore::new(conn.clone())),
            locks: Arc::new(SqliteLockStore::new(conn.clone())),
            namespaces: Arc::new(SqliteNamespaceStore::new(conn.clone())),
            orgs: Arc::new(SqliteOrganizationStore::new(conn.clone())),
            edges: Arc::new(SqliteEdgeStore::new(conn.clone())),
            event_query: Arc::new(SqliteEventQueryAdapter(SqliteEventQuery::new(conn.clone()))),
            users: Arc::new(SqliteUserStore::new(conn.clone())),
            memberships: Arc::new(SqliteOrgMembershipStore::new(conn)),
        }
    }

    fn build_pg_stores(backend: PgDatabase) -> Stores {
        use orchy_store_pg::*;
        let pool = backend.pool();
        let dims = backend.embedding_dimensions();
        Stores {
            agents: Arc::new(PgAgentStore::new(pool.clone())),
            tasks: Arc::new(PgTaskStore::new(pool.clone())),
            projects: Arc::new(PgProjectStore::new(pool.clone())),
            knowledge: Arc::new(PgKnowledgeStore::new(pool.clone(), dims)),
            messages: Arc::new(PgMessageStore::new(pool.clone())),
            locks: Arc::new(PgLockStore::new(pool.clone())),
            namespaces: Arc::new(PgNamespaceStore::new(pool.clone())),
            orgs: Arc::new(PgOrganizationStore::new(pool.clone())),
            edges: Arc::new(PgEdgeStore::new(pool.clone())),
            event_query: Arc::new(PgEventQueryAdapter(orchy_store_pg::PgEventQuery::new(
                pool.clone(),
            ))),
            users: Arc::new(PgUserStore::new(pool.clone())),
            memberships: Arc::new(PgOrgMembershipStore::new(pool)),
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
