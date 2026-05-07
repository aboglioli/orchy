use std::sync::Arc;

use orchy_application::{
    AddEdgeCommand, Application, ApplicationDeps, ApplicationError, ClaimTaskCommand,
    CompleteTaskCommand, GenerateApiKeyCommand, GetTaskCommand, ListAgentsCommand, PostTaskCommand,
    ReadKnowledgeCommand, RemoveEdgeCommand, RevokeApiKeyCommand, StartTaskCommand,
    WriteKnowledgeCommand,
};
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::api_key::ApiKeyStore;
use orchy_core::error::Error;
use orchy_core::graph::EdgeStore;
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::{OrgMembershipStore, UserStore};
use orchy_store_memory::*;

use orchy_server::auth::api_key::RandomApiKeyGenerator;
use orchy_server::auth::password::BcryptPasswordHasher;

fn mem() -> Arc<MemoryState> {
    Arc::new(MemoryState::new())
}

fn build_app(s: &Arc<MemoryState>) -> Application {
    Application::new(ApplicationDeps {
        agents: Arc::new(MemoryAgentStore::new(s.clone())) as Arc<dyn AgentStore>,
        tasks: Arc::new(MemoryTaskStore::new(s.clone())) as Arc<dyn TaskStore>,
        projects: Arc::new(MemoryProjectStore::new(s.clone())) as Arc<dyn ProjectStore>,
        knowledge: Arc::new(MemoryKnowledgeStore::new(s.clone())) as Arc<dyn KnowledgeStore>,
        messages: Arc::new(MemoryMessageStore::new(s.clone())) as Arc<dyn MessageStore>,
        locks: Arc::new(MemoryLockStore::new(s.clone())) as Arc<dyn LockStore>,
        namespaces: Arc::new(MemoryNamespaceStore::new(s.clone())),
        orgs: Arc::new(MemoryOrganizationStore::new(s.clone())),
        edges: Arc::new(MemoryEdgeStore::new(s.clone())) as Arc<dyn EdgeStore>,
        embeddings: None,
        reader_factory: Arc::new(MemoryReaderFactory::new(s.clone())),
        users: Arc::new(MemoryUserStore::new(s.clone())) as Arc<dyn UserStore>,
        memberships: Arc::new(MemoryOrgMembershipStore::new(s.clone()))
            as Arc<dyn OrgMembershipStore>,
        token_encoder: None,
        hasher: Arc::new(BcryptPasswordHasher::with_cost(4)),
        api_keys: Arc::new(MemoryApiKeyStore::new(s.clone())) as Arc<dyn ApiKeyStore>,
        api_key_generator: Arc::new(RandomApiKeyGenerator::new()),
    })
}

async fn register_agent(s: &Arc<MemoryState>, org: &str, alias: &str) -> AgentId {
    let org_id = OrganizationId::new(org).unwrap();
    let project = ProjectId::try_from("proj").unwrap();
    let alias = Alias::new(alias).unwrap();
    let mut agent = Agent::register(
        org_id,
        project,
        Namespace::root(),
        alias,
        vec![],
        String::new(),
        None,
        Default::default(),
        None,
    )
    .unwrap();
    let id = agent.id().clone();
    MemoryAgentStore::new(s.clone())
        .save(&mut agent)
        .await
        .unwrap();
    id
}

async fn create_task(app: &Application, org: &str) -> String {
    app.post_task
        .execute(PostTaskCommand {
            org_id: org.into(),
            project: "proj".into(),
            namespace: None,
            title: "task".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
            parent_id: None,
            depends_on: None,
        })
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn cross_org_get_task_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    let task_id = create_task(&app, "org-a").await;

    let result = app
        .get_task
        .execute(GetTaskCommand {
            task_id,
            org_id: "org-b".into(),
            relations: None,
        })
        .await;

    assert!(
        matches!(result, Err(ApplicationError::Core(Error::NotFound { .. }))),
        "cross-org get_task must return NotFound, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_claim_task_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    let task_id = create_task(&app, "org-a").await;
    let agent_id = register_agent(&s, "org-b", "agent-b").await;

    let result = app
        .claim_task
        .execute(ClaimTaskCommand {
            task_id,
            agent_id: agent_id.to_string(),
            org_id: "org-b".into(),
            start: None,
        })
        .await;

    assert!(
        matches!(result, Err(ApplicationError::Core(Error::NotFound { .. }))),
        "cross-org claim_task must return NotFound, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_start_task_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    let task_id = create_task(&app, "org-a").await;
    let agent_id = register_agent(&s, "org-b", "agent-b").await;

    let result = app
        .start_task
        .execute(StartTaskCommand {
            task_id,
            agent_id: agent_id.to_string(),
            org_id: "org-b".into(),
        })
        .await;

    assert!(
        matches!(result, Err(ApplicationError::Core(Error::NotFound { .. }))),
        "cross-org start_task must return NotFound, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_complete_task_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    let task_id = create_task(&app, "org-a").await;

    let result = app
        .complete_task
        .execute(CompleteTaskCommand {
            task_id,
            org_id: "org-b".into(),
            summary: None,
            links: vec![],
        })
        .await;

    assert!(
        matches!(result, Err(ApplicationError::Core(Error::NotFound { .. }))),
        "cross-org complete_task must return NotFound, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_remove_edge_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    let task_a = create_task(&app, "org-a").await;
    let task_b = create_task(&app, "org-a").await;

    let edge = app
        .add_edge
        .execute(AddEdgeCommand {
            org_id: "org-a".into(),
            from_kind: "task".into(),
            from_id: task_a,
            to_kind: "task".into(),
            to_id: task_b,
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    let result = app
        .remove_edge
        .execute(RemoveEdgeCommand {
            edge_id: edge.id,
            org_id: "org-b".into(),
        })
        .await;

    assert!(
        matches!(result, Err(ApplicationError::Core(Error::NotFound { .. }))),
        "cross-org remove_edge must return NotFound, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_revoke_api_key_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    let gen_resp = app
        .generate_api_key
        .execute(GenerateApiKeyCommand {
            org_id: "org-a".into(),
            user_id: None,
            name: "test-key".into(),
        })
        .await
        .unwrap();

    let api_keys = MemoryApiKeyStore::new(s.clone());
    let org_a = OrganizationId::new("org-a").unwrap();
    let keys = api_keys.find_by_org(&org_a).await.unwrap();
    let key_id = keys.first().unwrap().id().to_string();

    let _ = gen_resp;

    let result = app
        .revoke_api_key
        .execute(RevokeApiKeyCommand {
            key_id,
            org_id: "org-b".into(),
        })
        .await;

    assert!(
        matches!(result, Err(ApplicationError::Core(Error::NotFound { .. }))),
        "cross-org revoke_api_key must return NotFound, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_get_knowledge_returns_not_found() {
    let s = mem();
    let app = build_app(&s);

    app.write_knowledge
        .execute(WriteKnowledgeCommand {
            org_id: "org-a".into(),
            project: "proj".into(),
            namespace: None,
            path: "cross-test".into(),
            kind: "note".into(),
            title: "cross".into(),
            content: "secret".into(),
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

    let result = app
        .read_knowledge
        .execute(ReadKnowledgeCommand {
            org_id: "org-b".into(),
            project: "proj".into(),
            namespace: None,
            path: "cross-test".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(
        result.knowledge.is_none(),
        "cross-org read_knowledge must return knowledge=None, got: {result:?}"
    );
}

#[tokio::test]
async fn cross_org_list_agents_is_empty_for_other_org() {
    let s = mem();
    let app = build_app(&s);

    register_agent(&s, "org-a", "agent-a").await;

    let a_agents = app
        .list_agents
        .execute(ListAgentsCommand {
            org_id: "org-a".into(),
            project: None,
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(a_agents.items.len(), 1);

    let b_agents = app
        .list_agents
        .execute(ListAgentsCommand {
            org_id: "org-b".into(),
            project: None,
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    assert_eq!(b_agents.items.len(), 0, "org-b should not see org-a agents");
}
