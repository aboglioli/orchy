use std::sync::Arc;

use orchy_application::ApiKeyPrincipal;
use orchy_application::dto::OrganizationDto;

use crate::config::{AuthConfig, Config, ServerConfig, StoreConfig};
use crate::container::Container;
use crate::mcp::handler::OrchyHandler;
use crate::mcp::params::{
    ClaimTaskParams, CompleteTaskParams, PostTaskParams, QueryRelationsParams, RegisterAgentParams,
    SendMessageParams, WriteKnowledgeParams,
};

fn memory_config() -> Config {
    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 1,
            heartbeat_timeout_secs: 300,
            mcp_session_keep_alive_secs: None,
        },
        store: StoreConfig {
            backend: "memory".into(),
            sqlite: None,
            postgres: None,
        },
        auth: AuthConfig::default(),
        embeddings: None,
        skills: None,
    }
}

fn test_principal(org: &str) -> ApiKeyPrincipal {
    ApiKeyPrincipal {
        org: OrganizationDto {
            id: org.into(),
            name: format!("{org} org"),
            created_at: String::new(),
            updated_at: String::new(),
        },
        user_id: None,
    }
}

async fn make_handler(org: &str) -> (Arc<Container>, OrchyHandler) {
    let container = Container::from_config(memory_config()).await.unwrap();
    let auth = test_principal(org);
    let handler = OrchyHandler::new(container.clone(), auth).unwrap();
    (container, handler)
}

async fn register(h: &OrchyHandler, alias: &str, project: &str) -> String {
    let result = super::agent::register_agent(
        h,
        RegisterAgentParams {
            alias: alias.into(),
            project: project.into(),
            description: "test agent".into(),
            namespace: None,
            roles: Some(vec!["developer".into()]),
            agent_type: None,
            metadata: None,
        },
    )
    .await
    .expect("register_agent should succeed");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    parsed["agent"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn mcp_register_agent_sets_session_project_namespace() {
    let (_container, handler) = make_handler("default").await;

    let result = super::agent::register_agent(
        &handler,
        RegisterAgentParams {
            alias: "coder-1".into(),
            project: "proj".into(),
            description: "test coder".into(),
            namespace: Some("backend".into()),
            roles: None,
            agent_type: None,
            metadata: None,
        },
    )
    .await
    .expect("register_agent should succeed");

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["agent"]["alias"].as_str().unwrap(), "coder-1");
    assert_eq!(parsed["agent"]["namespace"].as_str().unwrap(), "/backend");

    let (agent_id, project, namespace) = handler.require_session().await.unwrap();
    assert_eq!(project.to_string(), "proj");
    assert_eq!(namespace.to_string(), "/backend");
    assert!(!agent_id.to_string().is_empty());
}

#[tokio::test]
async fn mcp_register_agent_idempotent_same_alias_resumes() {
    let (_container, handler) = make_handler("default").await;

    let result1 = super::agent::register_agent(
        &handler,
        RegisterAgentParams {
            alias: "resumer".into(),
            project: "proj".into(),
            description: "first".into(),
            namespace: None,
            roles: None,
            agent_type: None,
            metadata: None,
        },
    )
    .await
    .unwrap();
    let parsed1: serde_json::Value = serde_json::from_str(&result1).unwrap();
    let id1 = parsed1["agent"]["id"].as_str().unwrap().to_string();

    let result2 = super::agent::register_agent(
        &handler,
        RegisterAgentParams {
            alias: "resumer".into(),
            project: "proj".into(),
            description: "second".into(),
            namespace: None,
            roles: None,
            agent_type: None,
            metadata: None,
        },
    )
    .await
    .unwrap();
    let parsed2: serde_json::Value = serde_json::from_str(&result2).unwrap();
    let id2 = parsed2["agent"]["id"].as_str().unwrap().to_string();

    assert_eq!(id1, id2, "same alias must resume same agent UUID");
}

#[tokio::test]
async fn mcp_claim_task_uses_session_agent() {
    let (_container, handler) = make_handler("default").await;

    register(&handler, "claimer", "mcp-org").await;

    let post_result = super::task::post_task(
        &handler,
        PostTaskParams {
            namespace: None,
            title: "mcp task".into(),
            description: "test".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            parent_id: None,
            depends_on: None,
        },
    )
    .await
    .unwrap();
    let task: serde_json::Value = serde_json::from_str(&post_result).unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    let claim_result = super::task::claim_task(
        &handler,
        ClaimTaskParams {
            task_id: task_id.clone(),
            start: None,
        },
    )
    .await
    .unwrap();
    let claimed: serde_json::Value = serde_json::from_str(&claim_result).unwrap();
    assert_eq!(claimed["status"].as_str().unwrap(), "claimed");
}

#[tokio::test]
async fn mcp_query_relations_returns_neighborhood_with_linked_peers() {
    let (_container, handler) = make_handler("default").await;

    register(&handler, "graph-tester", "graph-proj").await;

    let post_result = super::task::post_task(
        &handler,
        PostTaskParams {
            namespace: None,
            title: "graph task".into(),
            description: "for graph test".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            parent_id: None,
            depends_on: None,
        },
    )
    .await
    .unwrap();
    let task: serde_json::Value = serde_json::from_str(&post_result).unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    super::knowledge::write_knowledge(
        &handler,
        WriteKnowledgeParams {
            path: "graph-decision".into(),
            kind: "decision".into(),
            title: "Graph Decision".into(),
            content: "decided".into(),
            namespace: None,
            tags: None,
            version: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: Some(task_id.clone()),
        },
    )
    .await
    .unwrap();

    let query_result = super::edge::query_relations(
        &handler,
        QueryRelationsParams {
            anchor_kind: "task".into(),
            anchor_id: task_id.clone(),
            rel_types: None,
            direction: None,
            max_depth: Some(1),
            limit: None,
            as_of: None,
            target_kinds: None,
            semantic_query: None,
            namespace: None,
            project: None,
        },
    )
    .await
    .unwrap();

    let neighborhood: serde_json::Value = serde_json::from_str(&query_result).unwrap();
    assert_eq!(neighborhood["anchor"]["kind"].as_str().unwrap(), "task");
    let relations = neighborhood["relations"].as_array().unwrap();
    assert!(
        !relations.is_empty(),
        "task with linked knowledge must have relations, got: {query_result}"
    );
    let has_produces = relations
        .iter()
        .any(|r| r["rel_type"].as_str() == Some("produces"));
    assert!(has_produces, "must find produces relation to knowledge");
}

#[tokio::test]
async fn mcp_complete_task_with_summary() {
    let (_container, handler) = make_handler("default").await;

    register(&handler, "completer", "complete-proj").await;

    let post_result = super::task::post_task(
        &handler,
        PostTaskParams {
            namespace: None,
            title: "completable task".into(),
            description: "test".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            parent_id: None,
            depends_on: None,
        },
    )
    .await
    .unwrap();
    let task: serde_json::Value = serde_json::from_str(&post_result).unwrap();
    let task_id = task["id"].as_str().unwrap().to_string();

    super::task::claim_task(
        &handler,
        ClaimTaskParams {
            task_id: task_id.clone(),
            start: Some(true),
        },
    )
    .await
    .unwrap();

    let result = super::task::complete_task(
        &handler,
        CompleteTaskParams {
            task_id: task_id.clone(),
            summary: Some("implemented with tests green".into()),
            links: None,
        },
    )
    .await
    .unwrap();

    let completed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(completed["status"].as_str().unwrap(), "completed");
    assert_eq!(
        completed["result_summary"].as_str().unwrap(),
        "implemented with tests green"
    );
}

#[tokio::test]
async fn mcp_register_agent_rejects_invalid_alias() {
    let (_container, handler) = make_handler("default").await;

    let err = super::agent::register_agent(
        &handler,
        RegisterAgentParams {
            alias: "x".into(),
            project: "proj".into(),
            description: "bad alias".into(),
            namespace: None,
            roles: None,
            agent_type: None,
            metadata: None,
        },
    )
    .await
    .expect_err("alias 'x' is too short and must be rejected");

    assert!(
        err.contains("validation failed:"),
        "error must contain 'invalid input:', got: {err}"
    );
    assert_eq!(
        err.matches("validation failed:").count(),
        1,
        "exactly one 'invalid input:' prefix expected, got: {err}"
    );
}

#[tokio::test]
async fn mcp_send_message_direct_delivery() {
    let (_container, handler) = make_handler("default").await;

    let receiver_id = register(&handler, "msg-receiver", "msg-proj").await;
    let sender_id = register(&handler, "msg-sender", "msg-proj").await;

    let result = super::message::send_message(
        &handler,
        SendMessageParams {
            to: receiver_id.clone(),
            body: "hello mcp".into(),
            namespace: None,
            reply_to: None,
            refs: None,
        },
    )
    .await
    .unwrap();

    let msg: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(msg["body"].as_str().unwrap(), "hello mcp");
    assert_eq!(msg["from"].as_str().unwrap(), sender_id);
}

#[tokio::test]
async fn mcp_write_knowledge_creates_entry() {
    let (_container, handler) = make_handler("default").await;

    register(&handler, "knower", "know-proj").await;

    let result = super::knowledge::write_knowledge(
        &handler,
        WriteKnowledgeParams {
            path: "test-entry".into(),
            kind: "decision".into(),
            title: "Test Decision".into(),
            content: "test content".into(),
            namespace: None,
            tags: Some(vec!["test".into()]),
            version: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        },
    )
    .await
    .unwrap();

    let entry: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(entry["path"].as_str().unwrap(), "test-entry");
    assert_eq!(entry["kind"].as_str().unwrap(), "decision");
    assert_eq!(entry["version"].as_u64().unwrap(), 1);
}
