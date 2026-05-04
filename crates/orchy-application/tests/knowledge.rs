use std::sync::Arc;

use orchy_application::{
    Application, ApplicationDeps, ArchiveKnowledgeCommand, DeleteKnowledgeCommand,
    ListKnowledgeCommand, ReadKnowledgeCommand, UnarchiveKnowledgeCommand, WriteKnowledgeCommand,
};
use orchy_core::agent::AgentStore;
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

fn write_cmd(path: &str, kind: &str, title: &str, content: &str) -> WriteKnowledgeCommand {
    WriteKnowledgeCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        path: path.into(),
        kind: kind.into(),
        title: title.into(),
        content: content.into(),
        tags: None,
        version: None,
        agent_id: None,
        metadata: None,
        metadata_remove: None,
        valid_from: None,
        valid_until: None,
        task_id: None,
    }
}

// ─── delete removes entry ───────────────────────────────────────────────────

#[tokio::test]
async fn delete_removes_entry() {
    let s = mem();
    let app = build_app(&s);

    app.write_knowledge
        .execute(write_cmd("delete-me", "note", "Delete Me", "bye"))
        .await
        .unwrap();

    app.delete_knowledge
        .execute(DeleteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "delete-me".into(),
        })
        .await
        .unwrap();

    use orchy_application::ReadKnowledgeDto;

    let result = app
        .read_knowledge
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "delete-me".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(result.knowledge.is_none(), "deleted entry knowledge should be None");
}

// ─── list filters by kind and tag ───────────────────────────────────────────

#[tokio::test]
async fn list_filters_by_kind_and_tag() {
    let s = mem();
    let app = build_app(&s);

    app.write_knowledge
        .execute(write_cmd("note-one", "note", "Note One", "note"))
        .await
        .unwrap();
    app.write_knowledge
        .execute(write_cmd("decision-one", "decision", "Decision One", "decision"))
        .await
        .unwrap();
    // Write note-one again with tags
    app.write_knowledge
        .execute(WriteKnowledgeCommand {
            tags: Some(vec!["important".into()]),
            ..write_cmd("note-one", "note", "Note One Updated", "note")
        })
        .await
        .unwrap();

    let notes = app.list_knowledge
        .execute(ListKnowledgeCommand {
            org_id: "default".into(),
            project: Some("test".into()),
            include_org_level: false,
            kind: Some("note".into()),
            tag: None,
            path_prefix: None,
            namespace: None,
            after: None,
            limit: None,
            orphaned: None,
            archived: None,
        })
        .await
        .unwrap();
    assert_eq!(notes.items.len(), 1, "expected 1 note entry");

    let important = app.list_knowledge
        .execute(ListKnowledgeCommand {
            org_id: "default".into(),
            project: Some("test".into()),
            include_org_level: false,
            kind: None,
            tag: Some("important".into()),
            path_prefix: None,
            namespace: None,
            after: None,
            limit: None,
            orphaned: None,
            archived: None,
        })
        .await
        .unwrap();
    assert_eq!(important.items.len(), 1, "expected 1 important entry");
    assert_eq!(important.items[0].path, "note-one");
}

// ─── archive and unarchive ─────────────────────────────────────────────────

#[tokio::test]
async fn archive_and_unarchive_knowledge() {
    let s = mem();
    let app = build_app(&s);

    app.write_knowledge
        .execute(write_cmd("archivable", "note", "Archivable", "content"))
        .await
        .unwrap();

    let archived = app
        .archive_knowledge
        .execute(ArchiveKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "archivable".into(),
            reason: Some("test".into()),
        })
        .await
        .unwrap();
    assert!(archived.archived);

    let restored = app
        .unarchive_knowledge
        .execute(UnarchiveKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "archivable".into(),
        })
        .await
        .unwrap();
    assert!(!restored.archived);
}

// ─── optimistic concurrency rejects stale version ───────────────────────────

#[tokio::test]
async fn optimistic_concurrency_rejects_stale_version() {
    let s = mem();
    let app = build_app(&s);

    // First write creates at version 1
    app.write_knowledge
        .execute(write_cmd("concurrent", "note", "v1", "first"))
        .await
        .unwrap();

    // Second write bumps to version 2
    app.write_knowledge
        .execute(write_cmd("concurrent", "note", "v2", "second"))
        .await
        .unwrap();

    // Third write with stale version 1 should fail
    let err = app
        .write_knowledge
        .execute(WriteKnowledgeCommand {
            version: Some(1),
            ..write_cmd("concurrent", "note", "v3", "third")
        })
        .await
        .expect_err("stale version should be rejected");
    assert!(
        matches!(err, Error::VersionMismatch { .. }),
        "expected VersionMismatch, got: {err:?}"
    );
}

// ─── temporal validity excludes expired entries ─────────────────────────────

#[tokio::test]
async fn temporal_validity_excludes_expired() {
    let s = mem();
    let app = build_app(&s);

    // Entry with valid_until in the past
    app.write_knowledge
        .execute(WriteKnowledgeCommand {
            valid_until: Some("2020-01-01T00:00:00Z".into()),
            ..write_cmd("expired-entry", "note", "Expired", "gone")
        })
        .await
        .unwrap();

    // Entry with no expiry
    app.write_knowledge
        .execute(write_cmd("active-entry", "note", "Active", "here"))
        .await
        .unwrap();

    let all = app.list_knowledge
        .execute(ListKnowledgeCommand {
            org_id: "default".into(),
            project: Some("test".into()),
            include_org_level: false,
            kind: None,
            tag: None,
            path_prefix: None,
            namespace: None,
            after: None,
            limit: None,
            orphaned: None,
            archived: None,
        })
        .await
        .unwrap();
    let paths: Vec<&str> = all.items.iter().map(|k| k.path.as_str()).collect();
    assert!(paths.contains(&"active-entry"), "active entry should be listed: {paths:?}");
    assert!(!paths.contains(&"expired-entry"), "expired entry should be excluded: {paths:?}");
}
