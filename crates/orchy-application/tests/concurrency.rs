use std::sync::Arc;

use orchy_application::{
    Application, ApplicationDeps, CheckLockCommand, LockResourceCommand, RegisterAgentCommand,
    UnlockResourceCommand,
};
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::api_key::{
    ApiKey, ApiKeyGenerator, ApiKeyPrefix, ApiKeyStore, ApiKeySuffix, HashedApiKey, PlainApiKey,
};
use orchy_core::error::{Error, Result};
use orchy_core::graph::EdgeStore;
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::organization::OrganizationId;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::{
    HashedPassword, OrgMembershipStore, PasswordHasher, PlainPassword, UserId, UserStore,
};
use orchy_store_memory::*;

struct NoopHasher;

impl PasswordHasher for NoopHasher {
    fn hash(&self, plain: &PlainPassword) -> Result<HashedPassword> {
        HashedPassword::new(plain.as_str())
    }
    fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> Result<()> {
        if plain.as_str() == hashed.as_str() {
            Ok(())
        } else {
            Err(Error::InvalidInput("password mismatch".into()))
        }
    }
}

struct NoopApiKeyGenerator;

impl ApiKeyGenerator for NoopApiKeyGenerator {
    fn generate(
        &self,
        org_id: &OrganizationId,
        user_id: Option<UserId>,
        name: String,
    ) -> Result<(PlainApiKey, ApiKey)> {
        let plain = PlainApiKey::new(
            "sk_0000000000000000000000000000000000000000000000000000000000000000".into(),
        )?;
        let hashed = HashedApiKey::new("noop-hash".into())?;
        let prefix = ApiKeyPrefix::new("sk_00000".into())?;
        let suffix = ApiKeySuffix::new("0000".into())?;
        let key = ApiKey::new(org_id.clone(), name, hashed, prefix, suffix, user_id);
        Ok((plain, key))
    }
    fn hash(&self, _plain: &PlainApiKey) -> HashedApiKey {
        HashedApiKey::new("noop-hash".into()).unwrap()
    }
}

fn mem() -> Arc<MemoryState> {
    Arc::new(MemoryState::new())
}

fn build_app(_s: &Arc<MemoryState>) -> Application {
    Application::new(ApplicationDeps {
        agents: Arc::new(MemoryAgentStore::new(_s.clone())) as Arc<dyn AgentStore>,
        tasks: Arc::new(MemoryTaskStore::new(_s.clone())) as Arc<dyn TaskStore>,
        projects: Arc::new(MemoryProjectStore::new(_s.clone())) as Arc<dyn ProjectStore>,
        knowledge: Arc::new(MemoryKnowledgeStore::new(_s.clone())) as Arc<dyn KnowledgeStore>,
        messages: Arc::new(MemoryMessageStore::new(_s.clone())) as Arc<dyn MessageStore>,
        locks: Arc::new(MemoryLockStore::new(_s.clone())) as Arc<dyn LockStore>,
        namespaces: Arc::new(MemoryNamespaceStore::new(_s.clone())),
        orgs: Arc::new(MemoryOrganizationStore::new(_s.clone())),
        edges: Arc::new(MemoryEdgeStore::new(_s.clone())) as Arc<dyn EdgeStore>,
        embeddings: None,
        reader_factory: Arc::new(MemoryReaderFactory::new(_s.clone())),
        users: Arc::new(MemoryUserStore::new(_s.clone())) as Arc<dyn UserStore>,
        memberships: Arc::new(MemoryOrgMembershipStore::new(_s.clone()))
            as Arc<dyn OrgMembershipStore>,
        token_encoder: None,
        hasher: Arc::new(NoopHasher),
        api_keys: Arc::new(MemoryApiKeyStore::new(_s.clone())) as Arc<dyn ApiKeyStore>,
        api_key_generator: Arc::new(NoopApiKeyGenerator),
    })
}

async fn register_app_agent(app: &Application, org: &str, alias: &str) -> AgentId {
    let resp = app
        .register_agent
        .execute(RegisterAgentCommand {
            org_id: org.into(),
            project: "proj".into(),
            namespace: None,
            alias: alias.into(),
            roles: vec!["dev".into()],
            description: String::new(),
            agent_type: None,
            metadata: Default::default(),
            auth_user_id: None,
        })
        .await
        .unwrap();
    resp.agent.id.parse().unwrap()
}

// ─── lock acquisition succeeds ─────────────────────────────────────────────

#[tokio::test]
async fn lock_acquire_and_release() {
    let s = mem();
    let app = build_app(&s);
    let agent = register_app_agent(&app, "default", "locker").await;

    app.lock_resource
        .execute(LockResourceCommand {
            org_id: "default".into(),
            project: "proj".into(),
            name: "my-lock".into(),
            holder_agent_id: agent.to_string(),
            ttl_secs: Some(60),
            namespace: None,
        })
        .await
        .unwrap();

    let check = app
        .check_lock
        .execute(CheckLockCommand {
            org_id: "default".into(),
            project: "proj".into(),
            name: "my-lock".into(),
            namespace: None,
        })
        .await
        .unwrap();
    assert!(check.is_some());

    app.unlock_resource
        .execute(UnlockResourceCommand {
            org_id: "default".into(),
            project: "proj".into(),
            name: "my-lock".into(),
            holder_agent_id: agent.to_string(),
            namespace: None,
        })
        .await
        .unwrap();

    let check_after = app
        .check_lock
        .execute(CheckLockCommand {
            org_id: "default".into(),
            project: "proj".into(),
            name: "my-lock".into(),
            namespace: None,
        })
        .await
        .unwrap();
    assert!(check_after.is_none(), "lock should be released");
}

// ─── lock contention returns conflict ───────────────────────────────────────

#[tokio::test]
async fn lock_contention_returns_conflict() {
    let s = mem();
    let app = build_app(&s);
    let agent_a = register_app_agent(&app, "default", "agent-a").await;
    let agent_b = register_app_agent(&app, "default", "agent-b").await;

    app.lock_resource
        .execute(LockResourceCommand {
            org_id: "default".into(),
            project: "proj".into(),
            name: "contended".into(),
            holder_agent_id: agent_a.to_string(),
            ttl_secs: Some(60),
            namespace: None,
        })
        .await
        .unwrap();

    let err = app
        .lock_resource
        .execute(LockResourceCommand {
            org_id: "default".into(),
            project: "proj".into(),
            name: "contended".into(),
            holder_agent_id: agent_b.to_string(),
            ttl_secs: Some(60),
            namespace: None,
        })
        .await
        .expect_err("lock contention should fail");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got: {err:?}"
    );
}
