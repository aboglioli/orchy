use std::sync::Arc;

use orchy_application::{
    Application, ApplicationDeps, CheckMailboxCommand, CheckSentMessagesCommand, MarkReadCommand,
    RegisterAgentCommand, SendMessageCommand,
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
        event_query: Arc::new(MemoryEventQuery::new(_s.clone())),
        users: Arc::new(MemoryUserStore::new(_s.clone())) as Arc<dyn UserStore>,
        memberships: Arc::new(MemoryOrgMembershipStore::new(_s.clone()))
            as Arc<dyn OrgMembershipStore>,
        token_encoder: None,
        hasher: Arc::new(NoopHasher),
        api_keys: Arc::new(MemoryApiKeyStore::new(_s.clone())) as Arc<dyn ApiKeyStore>,
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

// ─── DM delivers to addressed agent only ────────────────────────────────────

#[tokio::test]
async fn dm_delivers_to_addressed_agent_only() {
    let s = mem();
    let app = build_app(&s);
    let sender = register_app_agent(&app, "default", "sender", vec![]).await;
    let receiver = register_app_agent(&app, "default", "receiver", vec![]).await;
    let other = register_app_agent(&app, "default", "other", vec![]).await;

    app.send_message
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            from_agent_id: sender.to_string(),
            to: receiver.to_string(),
            body: "secret dm".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let inbox_receiver = app
        .check_mailbox
        .execute(CheckMailboxCommand {
            agent_id: receiver.to_string(),
            org_id: "default".into(),
            project: "proj".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert!(
        inbox_receiver.items.iter().any(|m| m.body == "secret dm"),
        "receiver should have the message"
    );

    let inbox_other = app
        .check_mailbox
        .execute(CheckMailboxCommand {
            agent_id: other.to_string(),
            org_id: "default".into(),
            project: "proj".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert!(
        !inbox_other.items.iter().any(|m| m.body == "secret dm"),
        "other agent should not receive the DM"
    );
}

// ─── role target delivers to all agents with role and excludes others ────────

#[tokio::test]
async fn role_target_delivers_to_all_with_role_and_excludes_others() {
    let s = mem();
    let app = build_app(&s);
    let sender = register_app_agent(&app, "default", "sender", vec![]).await;
    let dev1 = register_app_agent(&app, "default", "dev1", vec!["developer"]).await;
    let dev2 = register_app_agent(&app, "default", "dev2", vec!["developer"]).await;
    let ops = register_app_agent(&app, "default", "ops1", vec!["operator"]).await;

    app.send_message
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            from_agent_id: sender.to_string(),
            to: "role:developer".into(),
            body: "hello devs".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    for (id, name) in &[(dev1, "dev1"), (dev2, "dev2")] {
        let inbox = app
            .check_mailbox
            .execute(CheckMailboxCommand {
                agent_id: id.to_string(),
                org_id: "default".into(),
                project: "proj".into(),
                after: None,
                limit: None,
            })
            .await
            .unwrap();
        assert!(
            inbox.items.iter().any(|m| m.body == "hello devs"),
            "{name} should receive role message"
        );
    }

    let inbox_ops = app
        .check_mailbox
        .execute(CheckMailboxCommand {
            agent_id: ops.to_string(),
            org_id: "default".into(),
            project: "proj".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert!(
        !inbox_ops.items.iter().any(|m| m.body == "hello devs"),
        "ops should not receive developer role message"
    );
}

// ─── broadcast excludes sender ─────────────────────────────────────────────

#[tokio::test]
async fn broadcast_excludes_sender() {
    let s = mem();
    let app = build_app(&s);
    let sender = register_app_agent(&app, "default", "sender", vec![]).await;
    let other_a = register_app_agent(&app, "default", "other-a", vec![]).await;
    let other_b = register_app_agent(&app, "default", "other-b", vec![]).await;

    app.send_message
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            from_agent_id: sender.to_string(),
            to: "broadcast".into(),
            body: "hello everyone".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let inbox_self = app
        .check_mailbox
        .execute(CheckMailboxCommand {
            agent_id: sender.to_string(),
            org_id: "default".into(),
            project: "proj".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert!(
        !inbox_self.items.iter().any(|m| m.body == "hello everyone"),
        "sender should not receive own broadcast"
    );

    for id in &[other_a, other_b] {
        let inbox = app
            .check_mailbox
            .execute(CheckMailboxCommand {
                agent_id: id.to_string(),
                org_id: "default".into(),
                project: "proj".into(),
                after: None,
                limit: None,
            })
            .await
            .unwrap();
        assert!(
            inbox.items.iter().any(|m| m.body == "hello everyone"),
            "other agent should receive broadcast"
        );
    }
}

// ─── check sent messages returns sent messages ──────────────────────────────

#[tokio::test]
async fn check_sent_messages_returns_sent() {
    let s = mem();
    let app = build_app(&s);
    let sender = register_app_agent(&app, "default", "sender", vec![]).await;
    let receiver = register_app_agent(&app, "default", "receiver", vec![]).await;

    let msg = app
        .send_message
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            from_agent_id: sender.to_string(),
            to: receiver.to_string(),
            body: "sent check".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let sent = app
        .check_sent_messages
        .execute(CheckSentMessagesCommand {
            agent_id: sender.to_string(),
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert!(sent.items.iter().any(|m| m.id == msg.id));
}

// ─── mark read hides message from inbox ─────────────────────────────────────

#[tokio::test]
async fn mark_read_hides_dm_from_inbox() {
    let s = mem();
    let app = build_app(&s);
    let sender = register_app_agent(&app, "default", "sender", vec![]).await;
    let receiver = register_app_agent(&app, "default", "receiver", vec![]).await;

    let msg = app
        .send_message
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            from_agent_id: sender.to_string(),
            to: receiver.to_string(),
            body: "mark me read".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    app.mark_read
        .execute(MarkReadCommand {
            agent_id: receiver.to_string(),
            message_ids: vec![msg.id.clone()],
        })
        .await
        .unwrap();

    let inbox = app
        .check_mailbox
        .execute(CheckMailboxCommand {
            agent_id: receiver.to_string(),
            org_id: "default".into(),
            project: "proj".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();

    // Mark-read creates a receipt, hiding the message from default inbox queries
    let has_msg = inbox.items.iter().any(|m| m.id == msg.id);
    assert!(!has_msg, "DM should be hidden from inbox after mark-read");
}
