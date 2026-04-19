use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use orchy_application::Application;
use orchy_core::agent::AgentId;
use orchy_core::user::{OrgMembershipStore, PasswordHasher, UserStore};
use orchy_store_memory::MemoryBackend;
use orchy_store_pg::PgBackend;
use orchy_store_sqlite::SqliteBackend;

use crate::auth::{BcryptPasswordHasher, JwtTokenEncoder};
use crate::config::{Config, EmbeddingsConfig};
use crate::embeddings::{EmbeddingsBackend, OpenAiEmbeddingsProvider};
use crate::store::StoreBackend;

pub struct Container {
    pub store: Arc<StoreBackend>,
    pub app: Application,
    pub session_agents: Arc<RwLock<HashMap<String, AgentId>>>,
    pub config: Config,
    pub start_time: std::time::Instant,
    pub jwt_encoder: Option<JwtTokenEncoder>,
    pub password_hasher: Arc<dyn PasswordHasher>,
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

        use orchy_application::EventQuery;
        use orchy_core::agent::AgentStore;
        use orchy_core::edge::EdgeStore;
        use orchy_core::embeddings::EmbeddingsProvider;
        use orchy_core::knowledge::KnowledgeStore;
        use orchy_core::message::MessageStore;
        use orchy_core::namespace::NamespaceStore;
        use orchy_core::organization::OrganizationStore;
        use orchy_core::project::ProjectStore;
        use orchy_core::resource_lock::LockStore;
        use orchy_core::task::TaskStore;

        let app = Application::new(
            store.clone() as Arc<dyn AgentStore>,
            store.clone() as Arc<dyn TaskStore>,
            store.clone() as Arc<dyn ProjectStore>,
            store.clone() as Arc<dyn KnowledgeStore>,
            store.clone() as Arc<dyn MessageStore>,
            store.clone() as Arc<dyn LockStore>,
            store.clone() as Arc<dyn NamespaceStore>,
            store.clone() as Arc<dyn OrganizationStore>,
            store.clone() as Arc<dyn EdgeStore>,
            embeddings.map(|e| e as Arc<dyn EmbeddingsProvider>),
            store.clone() as Arc<dyn EventQuery>,
            store.clone() as Arc<dyn UserStore>,
            store.clone() as Arc<dyn OrgMembershipStore>,
        );

        let password_hasher: Arc<dyn PasswordHasher> =
            Arc::new(BcryptPasswordHasher::with_cost(config.auth.bcrypt_cost));

        let jwt_encoder = Self::init_jwt_encoder(&config).await?;

        let container = Arc::new(Self {
            store,
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
