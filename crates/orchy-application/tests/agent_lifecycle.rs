use std::sync::Arc;

use orchy_application::{
    Application, ApplicationDeps, ApplicationError, ChangeRolesCommand, GetAgentCommand,
    GetAgentSummaryCommand, ListAgentsCommand, PostTaskCommand, RegisterAgentCommand,
    RenameAliasCommand, ResolveAgentCommand, SwitchContextCommand,
};
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::api_key::{
    ApiKey, ApiKeyGenerator, ApiKeyPrefix, ApiKeyStore, ApiKeySuffix, HashedApiKey, PlainApiKey,
};
use orchy_core::error::{DomainError, DomainResult, Error, Result};
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
    fn hash(&self, plain: &PlainPassword) -> DomainResult<HashedPassword> {
        HashedPassword::new(plain.as_str())
    }

    fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> DomainResult<()> {
        if plain.as_str() == hashed.as_str() {
            Ok(())
        } else {
            Err(DomainError::PasswordMismatch)
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
        agents: Arc::new(MemoryAgentStore::new(Arc::clone(_s))) as Arc<dyn AgentStore>,
        tasks: Arc::new(MemoryTaskStore::new(Arc::clone(_s))) as Arc<dyn TaskStore>,
        projects: Arc::new(MemoryProjectStore::new(Arc::clone(_s))) as Arc<dyn ProjectStore>,
        knowledge: Arc::new(MemoryKnowledgeStore::new(Arc::clone(_s))) as Arc<dyn KnowledgeStore>,
        messages: Arc::new(MemoryMessageStore::new(Arc::clone(_s))) as Arc<dyn MessageStore>,
        locks: Arc::new(MemoryLockStore::new(Arc::clone(_s))) as Arc<dyn LockStore>,
        namespaces: Arc::new(MemoryNamespaceStore::new(Arc::clone(_s))),
        orgs: Arc::new(MemoryOrganizationStore::new(Arc::clone(_s))),
        edges: Arc::new(MemoryEdgeStore::new(Arc::clone(_s))) as Arc<dyn EdgeStore>,
        embeddings: None,
        reader_factory: Arc::new(MemoryReaderFactory::new(Arc::clone(_s))),
        users: Arc::new(MemoryUserStore::new(Arc::clone(_s))) as Arc<dyn UserStore>,
        memberships: Arc::new(MemoryOrgMembershipStore::new(Arc::clone(_s)))
            as Arc<dyn OrgMembershipStore>,
        token_encoder: None,
        hasher: Arc::new(NoopHasher),
        api_keys: Arc::new(MemoryApiKeyStore::new(Arc::clone(_s))) as Arc<dyn ApiKeyStore>,
        api_key_generator: Arc::new(NoopApiKeyGenerator),
    })
}

async fn register_app_agent(
    app: &Application,
    org: &str,
    alias: &str,
    roles: Vec<&str>,
) -> AgentId {
    let resp = app
        .register_agent
        .execute(RegisterAgentCommand {
            org_id: org.into(),
            project: "proj".into(),
            namespace: None,
            alias: alias.into(),
            roles: roles.iter().map(|r| r.to_string()).collect(),
            description: String::new(),
            agent_type: None,
            metadata: Default::default(),
            auth_user_id: None,
        })
        .await
        .unwrap();
    resp.agent.id.parse().unwrap()
}

// ─── register_agent idempotent on resume ────────────────────────────────────

#[tokio::test]
async fn register_agent_idempotent_on_resume() {
    let s = mem();
    let app = build_app(&s);

    let first = register_app_agent(&app, "org", "coder-1", vec!["dev"]).await;

    let second = register_app_agent(&app, "org", "coder-1", vec!["dev", "lead"]).await;

    assert_eq!(first, second, "resume must return same agent id");
}

// ─── change_roles replaces role set ─────────────────────────────────────────

#[tokio::test]
async fn change_roles_replaces_role_set() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = register_app_agent(&app, "org", "alpha", vec!["dev"]).await;

    app.change_roles
        .execute(ChangeRolesCommand {
            agent_id: agent_id.to_string(),
            roles: vec!["dev".into(), "lead".into()],
        })
        .await
        .unwrap();

    let agent = app
        .get_agent
        .execute(GetAgentCommand {
            agent_id: agent_id.to_string(),
            org_id: None,
            relations: None,
        })
        .await
        .unwrap();
    assert!(agent.roles.contains(&"lead".to_string()));
    assert!(agent.roles.contains(&"dev".to_string()));
    assert_eq!(agent.roles.len(), 2);
}

// ─── rename_alias unreachable by old alias ───────────────────────────────────

#[tokio::test]
async fn rename_alias_unreachable_by_old_alias() {
    let s = mem();
    let app = build_app(&s);
    let original_id = register_app_agent(&app, "org", "old-name", vec!["dev"]).await;

    app.rename_alias
        .execute(RenameAliasCommand {
            agent_id: original_id.to_string(),
            new_alias: "new-name".into(),
        })
        .await
        .unwrap();

    let resolve_old = app
        .resolve_agent
        .execute(ResolveAgentCommand {
            org_id: "org".into(),
            project: "proj".into(),
            id_or_alias: "old-name".into(),
        })
        .await;
    assert!(
        matches!(
            resolve_old,
            Err(ApplicationError::Core(Error::NotFound { .. }))
        ),
        "old alias should not resolve"
    );

    let resolved = app
        .resolve_agent
        .execute(ResolveAgentCommand {
            org_id: "org".into(),
            project: "proj".into(),
            id_or_alias: "new-name".into(),
        })
        .await
        .unwrap();
    assert_eq!(resolved.id.parse::<AgentId>().unwrap(), original_id);
}

// ─── switch_context changes namespace only ───────────────────────────────────

#[tokio::test]
async fn switch_context_changes_namespace_only() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = register_app_agent(&app, "org", "switcher", vec!["dev"]).await;

    let switched = app
        .switch_context
        .execute(SwitchContextCommand {
            org_id: "org".into(),
            agent_id: agent_id.to_string(),
            project: None,
            namespace: Some("/backend".into()),
        })
        .await
        .unwrap();
    assert_eq!(switched.namespace, "/backend");
}

// ─── get_agent_summary includes assigned tasks ──────────────────────────────

#[tokio::test]
async fn get_agent_summary_includes_assigned_tasks() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = register_app_agent(&app, "org", "summarized", vec![]).await;

    app.post_task
        .execute(PostTaskCommand {
            org_id: "org".into(),
            project: "proj".into(),
            namespace: None,
            title: "summary task".into(),
            description: "".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: Some(agent_id.to_string()),
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap();

    let summary = app
        .get_agent_summary
        .execute(GetAgentSummaryCommand {
            org_id: "org".into(),
            agent_id: agent_id.to_string(),
        })
        .await
        .unwrap();
    assert!(summary.counts.pending_tasks >= 1);
    assert_eq!(summary.agent.alias, "summarized");
}

// ─── list_agents returns agents for org ──────────────────────────────────────

#[tokio::test]
async fn list_agents_returns_agents_for_org() {
    let s = mem();
    let app = build_app(&s);
    register_app_agent(&app, "org", "alpha", vec!["dev"]).await;
    register_app_agent(&app, "org", "beta", vec!["ops"]).await;

    let page = app
        .list_agents
        .execute(ListAgentsCommand {
            org_id: "org".into(),
            project: None,
            after: None,
            limit: None,
        })
        .await
        .unwrap();

    let aliases: Vec<&str> = page.items.iter().map(|a| a.alias.as_str()).collect();
    assert!(aliases.contains(&"alpha"), "agents: {aliases:?}");
    assert!(aliases.contains(&"beta"), "agents: {aliases:?}");
}
