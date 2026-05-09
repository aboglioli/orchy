use std::sync::Arc;

use orchy_application::{
    AddEdgeCommand, Application, ApplicationDeps, ApplicationError, AssembleContextCommand,
    ListEdgesCommand, MaterializeNeighborhoodCommand, PostTaskCommand, RemoveEdgeCommand,
    WriteKnowledgeCommand,
};
use orchy_core::agent::AgentStore;
use orchy_core::api_key::{
    ApiKey, ApiKeyGenerator, ApiKeyPrefix, ApiKeyStore, ApiKeySuffix, HashedApiKey, PlainApiKey,
};
use orchy_core::error::{DomainError, DomainResult, Error, Result};
use orchy_core::graph::EdgeStore;
use orchy_core::graph::RelationOptions;
use orchy_core::graph::RelationType;
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

// ─── add edge creates relation ──────────────────────────────────────────────

#[tokio::test]
async fn add_edge_creates_relation() {
    let s = mem();
    let app = build_app(&s);

    let edge = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "task-a".into(),
            to_kind: "task".into(),
            to_id: "task-b".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();
    assert_eq!(edge.rel_type, "related_to");
    assert_eq!(edge.from_kind, "task");
    assert_eq!(edge.to_kind, "task");
}

// ─── add edge rejects duplicate (without if_not_exists) ──────────────────────

#[tokio::test]
async fn add_edge_rejects_duplicate() {
    let s = mem();
    let app = build_app(&s);

    app.add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "t1".into(),
            to_kind: "task".into(),
            to_id: "t2".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    let err = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "t1".into(),
            to_kind: "task".into(),
            to_id: "t2".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .expect_err("duplicate edge should be rejected");
    assert!(
        matches!(err, ApplicationError::Core(Error::Conflict(_))),
        "expected Conflict, got: {err:?}"
    );
}

// ─── add edge with if_not_exists returns existing edge ──────────────────────

#[tokio::test]
async fn add_edge_if_not_exists_returns_existing() {
    let s = mem();
    let app = build_app(&s);

    let first = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "t1".into(),
            to_kind: "task".into(),
            to_id: "t2".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    let second = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "t1".into(),
            to_kind: "task".into(),
            to_id: "t2".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: true,
        })
        .await
        .unwrap();
    assert_eq!(
        first.id, second.id,
        "if_not_exists should return the same edge id"
    );
}

// ─── list edges returns created edges ──────────────────────────────────────

#[tokio::test]
async fn list_edges_returns_created_edges() {
    let s = mem();
    let app = build_app(&s);

    let edge = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "t1".into(),
            to_kind: "task".into(),
            to_id: "t2".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    let page = app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: "default".into(),
            rel_type: None,
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert!(
        page.items.iter().any(|e| e.id == edge.id),
        "created edge should be listed"
    );
}

// ─── remove edge removes the relation ──────────────────────────────────────

#[tokio::test]
async fn remove_edge_removes_relation() {
    let s = mem();
    let app = build_app(&s);

    let edge = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: "t1".into(),
            to_kind: "task".into(),
            to_id: "t2".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    app.remove_edge
        .execute(RemoveEdgeCommand {
            edge_id: edge.id.clone(),
            org_id: "default".into(),
        })
        .await
        .unwrap();

    let page = app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: "default".into(),
            rel_type: None,
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert!(
        !page.items.iter().any(|e| e.id == edge.id),
        "removed edge should not be listed"
    );
}

// ─── add edge rejects self-cycle for depends_on ────────────────────────────
// ─── add edge self depends_on is allowed (no existing chain to cycle with) ──

#[tokio::test]
async fn add_edge_self_depends_on_succeeds() {
    let s = mem();
    let app = build_app(&s);

    let task = app
        .post_task
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "proj".into(),
            namespace: None,
            title: "self-cycle".into(),
            description: "".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap();

    let edge = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "task".into(),
            from_id: task.id.clone(),
            to_kind: "task".into(),
            to_id: task.id,
            rel_type: "depends_on".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .expect("self-depends_on edge succeeds (no pre-existing chain to cycle)");
    assert_eq!(edge.rel_type, "depends_on");
}

// ─── materialize_neighborhood returns peer summaries ───────────────────────

#[tokio::test]
async fn materialize_neighborhood_returns_peer_summaries() {
    let s = mem();
    let app = build_app(&s);
    let org_id = "default";

    // Create a task and knowledge entry, then connect them
    let task = app
        .post_task
        .execute(PostTaskCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            title: "neighborhood task".into(),
            description: "".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap();

    let knowledge = app
        .write_knowledge
        .execute(WriteKnowledgeCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            path: "neighborhood-test".into(),
            kind: "decision".into(),
            title: "Neighborhood Decision".into(),
            content: "neighbor".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    // Create edges: task produces knowledge
    app.add_edge
        .execute(AddEdgeCommand {
            org_id: org_id.into(),
            from_kind: "task".into(),
            from_id: task.id.clone(),
            to_kind: "knowledge".into(),
            to_id: knowledge.id.clone(),
            rel_type: "produces".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    let neighborhood = app
        .materialize_neighborhood
        .execute(MaterializeNeighborhoodCommand {
            org_id: org_id.into(),
            anchor_kind: "task".into(),
            anchor_id: task.id,
            options: RelationOptions::default(),
            as_of: None,
            project: Some("proj".into()),
            namespace: None,
            semantic_query: None,
        })
        .await
        .unwrap();
    assert_eq!(neighborhood.anchor.kind().to_string(), "task");
    assert!(
        neighborhood
            .relations
            .iter()
            .any(|r| matches!(r.rel_type, RelationType::Produces)),
        "neighborhood should include produces edge"
    );
}

// ─── assemble context returns structured context ────────────────────────────

#[tokio::test]
async fn assemble_context_returns_core_facts() {
    let s = mem();
    let app = build_app(&s);
    let org_id = "default";

    let task = app
        .post_task
        .execute(PostTaskCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            title: "context task".into(),
            description: "task for context".into(),
            acceptance_criteria: Some("must work".into()),
            priority: Some("high".into()),
            assigned_roles: None,
            created_by: None,
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap();

    let _knowledge = app
        .write_knowledge
        .execute(WriteKnowledgeCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            path: "context/decision".into(),
            kind: "decision".into(),
            title: "Context Decision".into(),
            content: "context decision body".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: Some(task.id.clone()),
        })
        .await
        .unwrap();

    let ctx = app
        .assemble_context
        .execute(AssembleContextCommand {
            org_id: org_id.into(),
            kind: "task".into(),
            id: task.id.clone(),
            max_tokens: Some(4000),
        })
        .await
        .unwrap();

    assert_eq!(ctx.root_kind, "task");
    assert_eq!(ctx.root_id, task.id);
    assert!(
        !ctx.core_facts.is_empty(),
        "should include linked knowledge as core facts"
    );
    assert!(
        ctx.core_facts.iter().any(|f| f.path == "context/decision"),
        "should include the linked decision"
    );
}

// ─── edge resolve knowledge path to UUID ────────────────────────────────────

#[tokio::test]
async fn edge_resolves_knowledge_path_to_uuid() {
    let s = mem();
    let app = build_app(&s);
    let org_id = "default";

    let k = app
        .write_knowledge
        .execute(WriteKnowledgeCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            path: "resolvable".into(),
            kind: "note".into(),
            title: "Resolvable".into(),
            content: "data".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let task = app
        .post_task
        .execute(PostTaskCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            title: "resolve task".into(),
            description: "".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap();

    let edge = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: org_id.into(),
            from_kind: "knowledge".into(),
            from_id: "resolvable".into(),
            to_kind: "task".into(),
            to_id: task.id.clone(),
            rel_type: "derived_from".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    assert_eq!(
        edge.from_id, k.id,
        "edge should store knowledge UUID, not path"
    );
}

// ─── edge query shows incoming derived_from for task ────────────────────────

#[tokio::test]
async fn query_relations_shows_incoming_derived_from_for_task() {
    let s = mem();
    let app = build_app(&s);
    let org_id = "default";

    let k = app
        .write_knowledge
        .execute(WriteKnowledgeCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            path: "derived-source".into(),
            kind: "note".into(),
            title: "Source".into(),
            content: "source".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let task = app
        .post_task
        .execute(PostTaskCommand {
            org_id: org_id.into(),
            project: "proj".into(),
            namespace: None,
            title: "derived task".into(),
            description: "".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap();

    app.add_edge
        .execute(AddEdgeCommand {
            org_id: org_id.into(),
            from_kind: "knowledge".into(),
            from_id: k.id.clone(),
            to_kind: "task".into(),
            to_id: task.id.clone(),
            rel_type: "derived_from".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    let neighborhood = app
        .materialize_neighborhood
        .execute(MaterializeNeighborhoodCommand {
            org_id: org_id.into(),
            anchor_kind: "task".into(),
            anchor_id: task.id.clone(),
            options: RelationOptions {
                rel_types: None,
                target_kinds: vec![],
                direction: orchy_core::graph::TraversalDirection::Incoming,
                max_depth: 1,
                limit: 50,
            },
            as_of: None,
            project: None,
            namespace: None,
            semantic_query: None,
        })
        .await
        .unwrap();

    assert!(
        neighborhood
            .relations
            .iter()
            .any(|r| matches!(r.rel_type, RelationType::DerivedFrom)),
        "should include incoming derived_from edge"
    );
}
