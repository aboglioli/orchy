use chrono::Utc;
use std::env;
use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use orchy_application::{CreateOrganizationCommand, GenerateApiKeyCommand};
use orchy_server::config::{AuthConfig, Config, ServerConfig, SqliteConfig, StoreConfig};
use orchy_server::container::Container;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;

fn test_config() -> Config {
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

async fn spawn_server() -> (String, String) {
    let container = Container::from_config(test_config()).await.unwrap();

    container
        .app
        .create_organization
        .execute(CreateOrganizationCommand {
            id: "test-org".into(),
            name: "Test Org".into(),
        })
        .await
        .unwrap();

    let key_resp = container
        .app
        .generate_api_key
        .execute(GenerateApiKeyCommand {
            org_id: "test-org".into(),
            user_id: None,
            name: "e2e-test-key".into(),
        })
        .await
        .unwrap();
    let api_key = key_resp.api_key;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let router = axum::Router::new()
        .nest("/api", orchy_server::api::router())
        .layer(CookieManagerLayer::new())
        .with_state(Arc::clone(&container));

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let base = format!("http://{addr}");
    (base, api_key)
}

#[tokio::test]
async fn full_agent_loop() {
    let (base, api_key) = spawn_server().await;

    let client_a = reqwest::Client::new();
    let client_b = reqwest::Client::new();

    let start_ts = Utc::now();

    let reg_a = client_a
        .post(format!("{base}/api/projects/smoke/agents"))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "alias": "agent-a",
            "description": "poster agent"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        reg_a.status(),
        200,
        "register agent-a failed: {}",
        reg_a.text().await.unwrap_or_default()
    );

    let reg_b = client_b
        .post(format!("{base}/api/projects/smoke/agents"))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "alias": "agent-b",
            "description": "worker agent"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        reg_b.status(),
        200,
        "register agent-b failed: {}",
        reg_b.text().await.unwrap_or_default()
    );
    let reg_b_body: serde_json::Value = reg_b.json().await.unwrap();
    let agent_b_id = reg_b_body["agent"]["id"].as_str().unwrap().to_string();

    let post_task_resp = client_a
        .post(format!("{base}/api/projects/smoke/tasks"))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "title": "implement feature",
            "description": "build the thing"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        post_task_resp.status(),
        200,
        "post task failed: {}",
        post_task_resp.text().await.unwrap_or_default()
    );
    let post_task_body: serde_json::Value = post_task_resp.json().await.unwrap();
    let task_id = post_task_body["id"].as_str().unwrap().to_string();
    assert_eq!(post_task_body["status"].as_str().unwrap(), "pending");

    let next_resp = client_b
        .get(format!("{base}/api/projects/smoke/tasks/next"))
        .bearer_auth(&api_key)
        .query(&[("agent_id", agent_b_id.as_str()), ("claim", "true")])
        .send()
        .await
        .unwrap();
    assert_eq!(
        next_resp.status(),
        200,
        "get next task failed: {}",
        next_resp.text().await.unwrap_or_default()
    );
    let next_body: serde_json::Value = next_resp.json().await.unwrap();
    assert_eq!(next_body["id"].as_str().unwrap(), task_id);
    assert_eq!(next_body["status"].as_str().unwrap(), "claimed");

    let start_resp = client_b
        .post(format!("{base}/api/projects/smoke/tasks/{task_id}/start"))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({"agent": agent_b_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        start_resp.status(),
        200,
        "start task failed: {}",
        start_resp.text().await.unwrap_or_default()
    );
    let start_body: serde_json::Value = start_resp.json().await.unwrap();
    assert_eq!(start_body["status"].as_str().unwrap(), "in_progress");

    let knowledge_resp = client_b
        .put(format!(
            "{base}/api/projects/smoke/knowledge/implementation-notes"
        ))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "kind": "decision",
            "title": "implementation approach",
            "content": "using direct approach",
            "task_id": task_id,
            "agent_id": agent_b_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        knowledge_resp.status(),
        200,
        "write knowledge failed: {}",
        knowledge_resp.text().await.unwrap_or_default()
    );

    let complete_resp = client_b
        .post(format!(
            "{base}/api/projects/smoke/tasks/{task_id}/complete"
        ))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({"summary": "implementation done"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        complete_resp.status(),
        200,
        "complete task failed: {}",
        complete_resp.text().await.unwrap_or_default()
    );
    let complete_body: serde_json::Value = complete_resp.json().await.unwrap();
    assert_eq!(complete_body["status"].as_str().unwrap(), "completed");

    let handoff_resp = client_b
        .put(format!("{base}/api/projects/smoke/knowledge/handoff"))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "kind": "context",
            "title": "session handoff",
            "content": "completed implement feature, no blockers"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        handoff_resp.status(),
        200,
        "write handoff failed: {}",
        handoff_resp.text().await.unwrap_or_default()
    );

    let task_check = client_a
        .get(format!("{base}/api/projects/smoke/tasks/{task_id}"))
        .bearer_auth(&api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(task_check.status(), 200, "get task failed");
    let task_final: serde_json::Value = task_check.json().await.unwrap();
    assert_eq!(task_final["status"].as_str().unwrap(), "completed");
    assert_eq!(
        task_final["result_summary"].as_str().unwrap(),
        "implementation done"
    );

    let since = start_ts.to_rfc3339();
    let events_resp = client_a
        .get(format!("{base}/api/projects/smoke/events"))
        .bearer_auth(&api_key)
        .query(&[("since", since.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(events_resp.status(), 200, "poll events failed");
    let events_body: serde_json::Value = events_resp.json().await.unwrap();
    let events = events_body["events"].as_array().unwrap();
    let topics: Vec<&str> = events.iter().filter_map(|e| e["topic"].as_str()).collect();

    assert!(
        topics.iter().any(|t| t.contains("task.created")),
        "missing task.created event, got: {topics:?}"
    );
    assert!(
        topics.iter().any(|t| t.contains("task.claimed")),
        "missing task.claimed event, got: {topics:?}"
    );
    assert!(
        topics.iter().any(|t| t.contains("task.started")),
        "missing task.started event, got: {topics:?}"
    );
    assert!(
        topics.iter().any(|t| t.contains("task.completed")),
        "missing task.completed event, got: {topics:?}"
    );
    assert!(
        topics.iter().any(|t| t.contains("knowledge")),
        "missing knowledge event, got: {topics:?}"
    );
}

fn sqlite_test_config(name: &str) -> Config {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = env::temp_dir().join(format!("orchy-{name}-{nonce}"));
    fs::create_dir_all(&dir).unwrap();

    Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 1,
            heartbeat_timeout_secs: 300,
            mcp_session_keep_alive_secs: None,
        },
        store: StoreConfig {
            backend: "sqlite".into(),
            sqlite: Some(SqliteConfig {
                path: dir.join("orchy.db").to_string_lossy().into_owned(),
            }),
            postgres: None,
        },
        auth: AuthConfig {
            jwt_duration_hours: 1,
            cookie_secure: false,
            bcrypt_cost: 4,
            keys_dir: dir.join("keys").to_string_lossy().into_owned(),
        },
        embeddings: None,
        skills: None,
    }
}

async fn spawn_sqlite_server(name: &str) -> String {
    let container = Container::from_config(sqlite_test_config(name))
        .await
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let router = axum::Router::new()
        .nest("/api", orchy_server::api::router())
        .layer(CookieManagerLayer::new())
        .with_state(Arc::clone(&container));

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn sqlite_auth_invite_new_user() {
    let base = spawn_sqlite_server("auth-invite").await;
    let client = reqwest::Client::new();

    let login_resp = client
        .post(format!("{base}/api/auth/login"))
        .json(&serde_json::json!({
            "email": "admin@orchy.sh",
            "password": "12345678"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp.status(), 200);
    let cookie = login_resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let login_body: serde_json::Value = login_resp.json().await.unwrap();
    assert!(
        login_body["memberships"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| {
                m["org_id"].as_str() == Some("default") && m["role"].as_str() == Some("owner")
            })
    );

    let invite_resp = client
        .post(format!("{base}/api/organizations/default/invite"))
        .header(reqwest::header::COOKIE, cookie)
        .json(&serde_json::json!({
            "email": "worker@example.com",
            "role": "member"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        invite_resp.status(),
        201,
        "invite failed: {}",
        invite_resp.text().await.unwrap_or_default()
    );
    let invite_body: serde_json::Value = invite_resp.json().await.unwrap();
    assert_eq!(
        invite_body["membership"]["org_id"].as_str().unwrap(),
        "default"
    );
    assert_eq!(
        invite_body["membership"]["role"].as_str().unwrap(),
        "member"
    );
}

// ─── Helper: boot a SQLite server and return an authenticated client + API key ─

struct TestContext {
    pub base: String,
    pub client: reqwest::Client,
    pub api_key: String,
    pub cookie: String,
}

async fn boot_authenticated(name: &str) -> TestContext {
    let base = spawn_sqlite_server(name).await;
    let client = reqwest::Client::new();

    let login_resp = client
        .post(format!("{base}/api/auth/login"))
        .json(&serde_json::json!({"email": "admin@orchy.sh", "password": "12345678"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login_resp.status(), 200);
    let cookie = login_resp
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(";")
        .next()
        .unwrap()
        .to_string();

    let key_resp = client
        .post(format!("{base}/api/api-keys"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({"name": format!("{name}-key")}))
        .send()
        .await
        .unwrap();
    assert_eq!(key_resp.status(), 200);
    let api_key = key_resp.json::<serde_json::Value>().await.unwrap()["api_key"]
        .as_str()
        .unwrap()
        .to_string();

    TestContext {
        base,
        client,
        api_key,
        cookie,
    }
}

// ─── SQLite: full bootstrap auth flow ──────────────────────────────────────

#[tokio::test]
async fn sqlite_bootstrap_full_auth_flow() {
    let ctx = boot_authenticated("full-auth").await;

    // Verify login returned membership with owner role on default org
    let me_resp = ctx
        .client
        .get(format!("{}/api/auth/me", ctx.base))
        .header(reqwest::header::COOKIE, &ctx.cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(me_resp.status(), 200);
    let me_body: serde_json::Value = me_resp.json().await.unwrap();
    assert!(
        me_body["memberships"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["org_id"].as_str() == Some("default") && m["role"].as_str() == Some("owner"))
    );

    // Use API key to register an agent
    let reg_resp = ctx
        .client
        .post(format!("{}/api/projects/smoke/agents", ctx.base))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({
            "alias": "bootstrap-agent",
            "description": "registered via key"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg_resp.status(), 200);
    let reg_body: serde_json::Value = reg_resp.json().await.unwrap();
    assert_eq!(
        reg_body["agent"]["alias"].as_str().unwrap(),
        "bootstrap-agent"
    );
}

// ─── SQLite: API key revoke invalidates key ─────────────────────────────────

#[tokio::test]
async fn sqlite_api_key_revoke_invalidates_key() {
    let ctx = boot_authenticated("key-revoke").await;

    // Get key ID from list (response is top-level array)
    let list_resp = ctx
        .client
        .get(format!("{}/api/api-keys", ctx.base))
        .bearer_auth(&ctx.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(
        list_resp.status(),
        200,
        "list keys failed: {}",
        list_resp.text().await.unwrap_or_default()
    );
    let list_body = list_resp.json::<serde_json::Value>().await.unwrap();
    let keys = list_body.as_array().unwrap_or_else(|| {
        panic!("expected array, got: {list_body}");
    });
    let key_id = keys
        .iter()
        .find(|k| k["name"].as_str().unwrap_or("").contains("key-revoke"))
        .map(|k| k["id"].as_str().unwrap())
        .unwrap()
        .to_string();

    // Verify key works (use a GET endpoint that exists)
    let verify_resp = ctx
        .client
        .get(format!("{}/api/projects/smoke", ctx.base))
        .bearer_auth(&ctx.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(verify_resp.status(), 200, "key should work before revoke");

    // Revoke (needs bearer auth)
    let revoke_resp = ctx
        .client
        .delete(format!("{}/api/api-keys/{key_id}", ctx.base))
        .bearer_auth(&ctx.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), 204);

    // Key should now be rejected (use a GET endpoint)
    let after_resp = ctx
        .client
        .get(format!("{}/api/projects/smoke", ctx.base))
        .bearer_auth(&ctx.api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(
        after_resp.status(),
        401,
        "revoked key must return 401, got {}",
        after_resp.status()
    );
}

// ─── SQLite: invalid API key returns 401 ────────────────────────────────────

#[tokio::test]
async fn sqlite_invalid_api_key_returns_401() {
    let ctx = boot_authenticated("bad-key").await;

    let resp = ctx
        .client
        .get(format!("{}/api/projects/smoke", ctx.base))
        .bearer_auth("sk_0000000000000000000000000000000000000000000000000000000000000000")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

// ─── SQLite: agent registers, creates task, claims, completes ───────────────

#[tokio::test]
async fn sqlite_agent_task_lifecycle() {
    let ctx = boot_authenticated("agent-task").await;
    let base = &ctx.base;
    let key = &ctx.api_key;

    let reg = ctx
        .client
        .post(format!("{base}/api/projects/e2e/agents"))
        .bearer_auth(key)
        .json(&serde_json::json!({
            "alias": "worker",
            "description": "e2e worker",
            "roles": ["developer"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg.status(), 200);
    let reg_body: serde_json::Value = reg.json().await.unwrap();
    let agent_id = reg_body["agent"]["id"].as_str().unwrap().to_string();
    assert_eq!(reg_body["agent"]["alias"].as_str().unwrap(), "worker");

    let task = ctx
        .client
        .post(format!("{base}/api/projects/e2e/tasks"))
        .bearer_auth(key)
        .json(&serde_json::json!({
            "title": "e2e task",
            "description": "test lifecycle",
            "roles": ["developer"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(task.status(), 200);
    let task_body: serde_json::Value = task.json().await.unwrap();
    let task_id = task_body["id"].as_str().unwrap().to_string();
    assert_eq!(task_body["status"].as_str().unwrap(), "pending");
    assert!(
        task_body.get("created_by").is_some(),
        "created_by must be non-null"
    );

    let claim = ctx
        .client
        .post(format!("{base}/api/projects/e2e/tasks/{task_id}/claim"))
        .bearer_auth(key)
        .json(&serde_json::json!({"agent": agent_id, "start": true}))
        .send()
        .await
        .unwrap();
    assert_eq!(claim.status(), 200);
    let claim_body: serde_json::Value = claim.json().await.unwrap();
    assert_eq!(claim_body["status"].as_str().unwrap(), "in_progress");

    let complete = ctx
        .client
        .post(format!("{base}/api/projects/e2e/tasks/{task_id}/complete"))
        .bearer_auth(key)
        .json(&serde_json::json!({"summary": "done"}))
        .send()
        .await
        .unwrap();
    assert_eq!(complete.status(), 200);
    let complete_body: serde_json::Value = complete.json().await.unwrap();
    assert_eq!(complete_body["status"].as_str().unwrap(), "completed");

    let get = ctx
        .client
        .get(format!("{base}/api/projects/e2e/tasks/{task_id}"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert_eq!(get.status(), 200);
    let get_body: serde_json::Value = get.json().await.unwrap();
    assert_eq!(get_body["status"].as_str().unwrap(), "completed");

    let unknown = ctx
        .client
        .get(format!(
            "{base}/api/projects/e2e/tasks/00000000-0000-0000-0000-000000000000",
        ))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
}

// ─── SQLite: cascade unblock ───────────────────────────────────────────────

#[tokio::test]
async fn sqlite_dependent_task_cascade_unblock() {
    let ctx = boot_authenticated("cascade").await;
    let base = &ctx.base;
    let key = &ctx.api_key;

    let reg = ctx
        .client
        .post(format!("{base}/api/projects/cas/agents"))
        .bearer_auth(key)
        .json(&serde_json::json!({"alias": "worker", "description": ""}))
        .send()
        .await
        .unwrap();
    let agent_id = reg.json::<serde_json::Value>().await.unwrap()["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let base_task = ctx
        .client
        .post(format!("{base}/api/projects/cas/tasks"))
        .bearer_auth(key)
        .json(&serde_json::json!({"title": "base", "description": ""}))
        .send()
        .await
        .unwrap();
    let base_id = base_task.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let dep = ctx
        .client
        .post(format!("{base}/api/projects/cas/tasks"))
        .bearer_auth(key)
        .json(&serde_json::json!({
            "title": "dependent",
            "description": "",
            "depends_on": [base_id]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(dep.status(), 200);
    let dep_body: serde_json::Value = dep.json().await.unwrap();
    assert_eq!(dep_body["status"].as_str().unwrap(), "blocked");
    let dep_id = dep_body["id"].as_str().unwrap().to_string();

    ctx.client
        .post(format!("{base}/api/projects/cas/tasks/{base_id}/claim"))
        .bearer_auth(key)
        .json(&serde_json::json!({"agent": agent_id, "start": true}))
        .send()
        .await
        .unwrap();
    ctx.client
        .post(format!("{base}/api/projects/cas/tasks/{base_id}/complete"))
        .bearer_auth(key)
        .json(&serde_json::json!({"summary": "done"}))
        .send()
        .await
        .unwrap();

    let dep_get = ctx
        .client
        .get(format!("{base}/api/projects/cas/tasks/{dep_id}"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    let dep_status = dep_get.json::<serde_json::Value>().await.unwrap();
    assert_eq!(dep_status["status"].as_str().unwrap(), "pending");
}

// ─── SQLite: split auto-completes parent ────────────────────────────────────

#[tokio::test]
async fn sqlite_split_subtasks_auto_complete_parent() {
    let ctx = boot_authenticated("split").await;
    let base = &ctx.base;
    let key = &ctx.api_key;

    let reg = ctx
        .client
        .post(format!("{base}/api/projects/split/agents"))
        .bearer_auth(key)
        .json(&serde_json::json!({"alias": "worker", "description": ""}))
        .send()
        .await
        .unwrap();
    let agent_id = reg.json::<serde_json::Value>().await.unwrap()["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let parent = ctx
        .client
        .post(format!("{base}/api/projects/split/tasks"))
        .bearer_auth(key)
        .json(&serde_json::json!({"title": "parent", "description": ""}))
        .send()
        .await
        .unwrap();
    let parent_id = parent.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    ctx.client
        .post(format!("{base}/api/projects/split/tasks/{parent_id}/claim"))
        .bearer_auth(key)
        .json(&serde_json::json!({"agent": agent_id, "start": true}))
        .send()
        .await
        .unwrap();

    let split = ctx
        .client
        .post(format!("{base}/api/projects/split/tasks/{parent_id}/split"))
        .bearer_auth(key)
        .json(&serde_json::json!({"subtasks": [
            {"title": "child-one", "description": ""},
            {"title": "child-two", "description": ""}
        ]}))
        .send()
        .await
        .unwrap();
    assert_eq!(split.status(), 200);
    let split_body: serde_json::Value = split.json().await.unwrap();
    let children = split_body["subtasks"].as_array().unwrap().clone();
    assert_eq!(children.len(), 2);

    for child in &children {
        let cid = child["id"].as_str().unwrap();
        ctx.client
            .post(format!("{base}/api/projects/split/tasks/{cid}/claim"))
            .bearer_auth(key)
            .json(&serde_json::json!({"agent": agent_id, "start": true}))
            .send()
            .await
            .unwrap();
        ctx.client
            .post(format!("{base}/api/projects/split/tasks/{cid}/complete"))
            .bearer_auth(key)
            .json(&serde_json::json!({"summary": "done"}))
            .send()
            .await
            .unwrap();
    }

    let parent_get = ctx
        .client
        .get(format!("{base}/api/projects/split/tasks/{parent_id}"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert_eq!(parent_get.status(), 200);
    let parent_body: serde_json::Value = parent_get.json().await.unwrap();
    assert_eq!(parent_body["status"].as_str().unwrap(), "completed");
}

// ─── SQLite: knowledge write, read, delete ──────────────────────────────────

#[tokio::test]
async fn sqlite_knowledge_write_read_delete() {
    let ctx = boot_authenticated("knowledge-rw").await;
    let base = &ctx.base;
    let key = &ctx.api_key;

    let reg = ctx
        .client
        .post(format!("{base}/api/projects/kproj/agents"))
        .bearer_auth(key)
        .json(&serde_json::json!({"alias": "knower", "description": ""}))
        .send()
        .await
        .unwrap();
    let agent_id = reg.json::<serde_json::Value>().await.unwrap()["agent"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let write = ctx
        .client
        .put(format!("{base}/api/projects/kproj/knowledge/test-entry"))
        .bearer_auth(key)
        .json(&serde_json::json!({
            "kind": "decision",
            "title": "Test Decision",
            "content": "content body",
            "agent_id": agent_id,
            "tags": ["test"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(write.status(), 200);
    let write_body: serde_json::Value = write.json().await.unwrap();
    assert_eq!(write_body["path"].as_str().unwrap(), "test-entry");
    assert_eq!(write_body["kind"].as_str().unwrap(), "decision");

    let read = ctx
        .client
        .get(format!("{base}/api/projects/kproj/knowledge/test-entry"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);
    let read_body: serde_json::Value = read.json().await.unwrap();
    assert_eq!(read_body["content"].as_str().unwrap(), "content body");

    let delete = ctx
        .client
        .delete(format!("{base}/api/projects/kproj/knowledge/test-entry"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert!(delete.status().is_success(), "delete should succeed");

    // Read after delete returns the entry (delete archives, doesn't fully remove)
    let read_after = ctx
        .client
        .get(format!("{base}/api/projects/kproj/knowledge/test-entry"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert!(
        read_after.status().is_success(),
        "read after delete: got {}",
        read_after.status()
    );
}

// ─── SQLite: message send, inbox, sent, mark-read ───────────────────────────

#[tokio::test]
async fn sqlite_message_send_inbox_sent_mark_read() {
    let ctx = boot_authenticated("msg-flow").await;
    let base = &ctx.base;
    let key = &ctx.api_key;

    let a1 = ctx
        .client
        .post(format!("{base}/api/projects/msgproj/agents"))
        .bearer_auth(key)
        .json(&serde_json::json!({"alias": "sender", "description": ""}))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let a1_id = a1["agent"]["id"].as_str().unwrap().to_string();

    let a2 = ctx
        .client
        .post(format!("{base}/api/projects/msgproj/agents"))
        .bearer_auth(key)
        .json(&serde_json::json!({"alias": "receiver", "description": ""}))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let a2_id = a2["agent"]["id"].as_str().unwrap().to_string();
    let empty = vec![];

    let msg = ctx
        .client
        .post(format!("{base}/api/projects/msgproj/messages"))
        .bearer_auth(key)
        .json(&serde_json::json!({
            "from_alias": a1_id,
            "to": a2_id,
            "body": "hello from e2e"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(msg.status(), 200);
    let msg_body: serde_json::Value = msg.json().await.unwrap();
    let msg_id = msg_body["id"].as_str().unwrap().to_string();

    let sent = ctx
        .client
        .get(format!(
            "{base}/api/agents/{a1_id}/sent-messages?project=msgproj"
        ))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert_eq!(sent.status(), 200);
    let sent_body: serde_json::Value = sent.json().await.unwrap();
    assert!(
        sent_body["items"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .any(|m| m["id"].as_str() == Some(&msg_id))
    );

    let inbox = ctx
        .client
        .get(format!("{base}/api/agents/{a2_id}/inbox?project=msgproj"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    assert_eq!(inbox.status(), 200);
    let inbox_body: serde_json::Value = inbox.json().await.unwrap();
    assert!(
        inbox_body["items"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .any(|m| m["body"].as_str() == Some("hello from e2e"))
    );

    let read = ctx
        .client
        .post(format!(
            "{base}/api/agents/{a2_id}/messages/read?project=msgproj"
        ))
        .bearer_auth(key)
        .json(&serde_json::json!({"message_ids": [msg_id]}))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), 200);

    let inbox_after = ctx
        .client
        .get(format!("{base}/api/agents/{a2_id}/inbox?project=msgproj"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap();
    let inbox_after_body: serde_json::Value = inbox_after.json().await.unwrap();
    let empty = vec![];
    let read_msg = inbox_after_body["items"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .find(|m| m["body"].as_str() == Some("hello from e2e"));
    assert!(
        read_msg.is_some(),
        "DM should still be in inbox after mark-read"
    );
    assert_eq!(
        read_msg.unwrap()["status"].as_str().unwrap(),
        "read",
        "DM should have status 'read' after mark-read"
    );
}

// ─── SQLite: migrations idempotent on existing DB ───────────────────────────

#[tokio::test]
async fn sqlite_migrations_idempotent_on_existing_db() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = env::temp_dir().join(format!("orchy-migrate-idem-{nonce}"));
    fs::create_dir_all(&dir).unwrap();

    let make_config = || Config {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 1,
            heartbeat_timeout_secs: 300,
            mcp_session_keep_alive_secs: None,
        },
        store: StoreConfig {
            backend: "sqlite".into(),
            sqlite: Some(SqliteConfig {
                path: dir.join("orchy.db").to_string_lossy().into_owned(),
            }),
            postgres: None,
        },
        auth: AuthConfig {
            jwt_duration_hours: 1,
            cookie_secure: false,
            bcrypt_cost: 4,
            keys_dir: dir.join("keys").to_string_lossy().into_owned(),
        },
        embeddings: None,
        skills: None,
    };

    let container1 = Container::from_config(make_config()).await.unwrap();
    let listener1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr1 = listener1.local_addr().unwrap();
    let router1 = axum::Router::new()
        .nest("/api", orchy_server::api::router())
        .layer(CookieManagerLayer::new())
        .with_state(Arc::clone(&container1));
    let server1 = tokio::spawn(async move {
        axum::serve(listener1, router1).await.unwrap();
    });

    let base1 = format!("http://{addr1}");
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{base1}/api/auth/login"))
        .json(&serde_json::json!({"email": "admin@orchy.sh", "password": "12345678"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    server1.abort();
    drop(container1);
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let container2 = Container::from_config(make_config()).await.unwrap();
    let listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap();
    let router2 = axum::Router::new()
        .nest("/api", orchy_server::api::router())
        .layer(CookieManagerLayer::new())
        .with_state(Arc::clone(&container2));
    let server2 = tokio::spawn(async move {
        axum::serve(listener2, router2).await.unwrap();
    });

    let base2 = format!("http://{addr2}");
    let resp2 = client
        .post(format!("{base2}/api/auth/login"))
        .json(&serde_json::json!({"email": "admin@orchy.sh", "password": "12345678"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp2.status(),
        200,
        "second boot on existing DB must succeed: {}",
        resp2.text().await.unwrap_or_default()
    );

    server2.abort();
    drop(container2);
    let _ = fs::remove_dir_all(&dir);
}

// ─── CLI binary helper ──────────────────────────────────────────────────────

fn workspace_root() -> std::path::PathBuf {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn orchy_bin() -> std::path::PathBuf {
    use std::sync::OnceLock;
    static BIN: OnceLock<std::path::PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        if let Ok(path) = std::env::var("CARGO_BIN_EXE_orchy") {
            let bin = std::path::PathBuf::from(path);
            if bin.exists() {
                return bin;
            }
        }
        let root = workspace_root();
        let bin = root.join("target").join("debug").join("orchy");
        if bin.exists() {
            return bin;
        }
        let status = std::process::Command::new("cargo")
            .current_dir(&root)
            .args(["build", "-p", "orchy-cli"])
            .status()
            .expect("failed to invoke cargo build -p orchy-cli");
        assert!(status.success(), "cargo build -p orchy-cli failed");
        assert!(
            bin.exists(),
            "orchy binary still missing after build: {bin:?}"
        );
        bin
    })
    .clone()
}

fn cli_config(
    dir: &std::path::Path,
    base_url: &str,
    api_key: &str,
    alias: &str,
    project: &str,
) -> std::path::PathBuf {
    let config_path = dir.join(".orchy.toml");
    let config_toml = format!(
        "url = \"{base_url}\"\napi_key = \"{api_key}\"\nproject = \"{project}\"\nalias = \"{alias}\"\n"
    );
    fs::write(&config_path, config_toml).unwrap();
    config_path
}

/// Run orchy CLI with the given args. Config must be in the temp dir's .orchy.toml.
async fn run_orchy_in_dir(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    let bin = orchy_bin();
    tokio::process::Command::new(bin)
        .current_dir(dir)
        .args(args)
        .output()
        .await
        .unwrap()
}

/// Run orchy CLI with url/api-key/project flags, no config file needed.
async fn run_orchy_with_flags(
    base: &str,
    key: &str,
    project: &str,
    agent: &str,
    args: &[&str],
) -> std::process::Output {
    let bin = orchy_bin();
    let mut all_args = vec![
        "--url",
        base,
        "--api-key",
        key,
        "--project",
        project,
        "--agent",
        agent,
    ];
    all_args.extend_from_slice(args);
    tokio::process::Command::new(bin)
        .args(&all_args)
        .output()
        .await
        .unwrap()
}

// ─── CLI: agent register does not clobber explicit alias ────────────────────

#[tokio::test]
async fn sqlite_cli_agent_register_does_not_clobber_explicit_alias() {
    let ctx = boot_authenticated("cli-register").await;
    let dir = env::temp_dir().join("orchy-cli-register");
    fs::create_dir_all(&dir).unwrap();
    let config_path = cli_config(&dir, &ctx.base, &ctx.api_key, "preset-alias", "anyproj");

    let output = run_orchy_in_dir(
        &dir,
        &[
            "--agent",
            "cli-newone",
            "agent",
            "register",
            "--description",
            "test",
        ],
    )
    .await;
    assert!(
        output.status.success(),
        "register failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config_content = fs::read_to_string(&config_path).unwrap();
    assert!(
        config_content.contains("preset-alias"),
        ".orchy.toml must preserve 'preset-alias':\n{config_content}"
    );
    assert!(
        !config_content.contains("cli-newone"),
        ".orchy.toml must not contain 'cli-newone':\n{config_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}

// ─── CLI: agent list plaintext shows agents ──────────────────────────────────

#[tokio::test]
async fn sqlite_cli_agent_list_plaintext_shows_agents() {
    let ctx = boot_authenticated("cli-list").await;

    ctx.client
        .post(format!("{}/api/projects/anyproj/agents", ctx.base))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({"alias": "alice", "description": ""}))
        .send()
        .await
        .unwrap();
    ctx.client
        .post(format!("{}/api/projects/anyproj/agents", ctx.base))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({"alias": "bob", "description": ""}))
        .send()
        .await
        .unwrap();

    let output = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "cli-agent",
        &["agent", "list"],
    )
    .await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "agent list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("alice"),
        "stdout must contain 'alice':\n{stdout}"
    );
    assert!(
        stdout.contains("bob"),
        "stdout must contain 'bob':\n{stdout}"
    );
}

// ─── CLI: knowledge read plaintext shows content ────────────────────────────

#[tokio::test]
async fn sqlite_cli_knowledge_read_plaintext_shows_content() {
    let ctx = boot_authenticated("cli-know").await;

    ctx.client
        .put(format!(
            "{}/api/projects/anyproj/knowledge/test-note",
            ctx.base
        ))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({
            "kind": "note",
            "title": "Test Note",
            "content": "hello world content"
        }))
        .send()
        .await
        .unwrap();

    let output = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "cli-agent",
        &["knowledge", "read", "test-note"],
    )
    .await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "knowledge read failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("hello world content"),
        "stdout must contain content:\n{stdout}"
    );
}

// ─── CLI: lock acquire release with path ────────────────────────────────────

#[tokio::test]
async fn sqlite_cli_lock_acquire_release_with_path() {
    let ctx = boot_authenticated("cli-lock").await;

    ctx.client
        .post(format!("{}/api/projects/anyproj/agents", ctx.base))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({"alias": "cli-agent", "description": ""}))
        .send()
        .await
        .unwrap();

    let acquire = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "cli-agent",
        &["lock", "acquire", "src/auth.rs", "--ttl", "60"],
    )
    .await;
    assert!(
        acquire.status.success(),
        "lock acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );

    let check = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "cli-agent",
        &["lock", "check", "src/auth.rs"],
    )
    .await;
    assert!(
        check.status.success(),
        "lock check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let release = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "cli-agent",
        &["lock", "release", "src/auth.rs"],
    )
    .await;
    assert!(
        release.status.success(),
        "lock release failed: {}",
        String::from_utf8_lossy(&release.stderr)
    );
}

// ─── CLI: task create records created_by ────────────────────────────────────

#[tokio::test]
async fn sqlite_cli_task_create_records_created_by() {
    let ctx = boot_authenticated("cli-task").await;

    ctx.client
        .post(format!("{}/api/projects/anyproj/agents", ctx.base))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({"alias": "task-author", "description": ""}))
        .send()
        .await
        .unwrap();

    let output = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "task-author",
        &[
            "task",
            "create",
            "--title",
            "cli task",
            "--description",
            "test",
            "--json",
        ],
    )
    .await;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "task create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let task: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        task.get("created_by").is_some(),
        "created_by must be non-null: {stdout}"
    );
}

// ─── CLI: task add-dep and remove-dep ───────────────────────────────────────

#[tokio::test]
async fn sqlite_cli_task_add_remove_dep() {
    let ctx = boot_authenticated("cli-add-remove-dep").await;

    ctx.client
        .post(format!("{}/api/projects/anyproj/agents", ctx.base))
        .bearer_auth(&ctx.api_key)
        .json(&serde_json::json!({"alias": "dep-worker", "description": ""}))
        .send()
        .await
        .unwrap();

    // Create two tasks
    let task_a = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &[
            "task",
            "create",
            "--title",
            "Task A",
            "--description",
            "A",
            "--json",
        ],
    )
    .await;
    assert!(task_a.status.success(), "task A create failed");
    let task_a_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&task_a.stdout)).unwrap();
    let id_a = task_a_json["id"].as_str().unwrap();

    let task_b = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &[
            "task",
            "create",
            "--title",
            "Task B",
            "--description",
            "B",
            "--json",
        ],
    )
    .await;
    assert!(task_b.status.success(), "task B create failed");
    let task_b_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&task_b.stdout)).unwrap();
    let id_b = task_b_json["id"].as_str().unwrap();

    // Add dependency: B depends on A
    let add_out = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "add-dep", id_b, "--dep", id_a, "--json"],
    )
    .await;
    assert!(add_out.status.success(), "add-dep failed");
    let add_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&add_out.stdout)).unwrap();
    assert_eq!(
        add_json["status"].as_str(),
        Some("blocked"),
        "task B must be blocked after adding dependency"
    );

    // Complete task A
    let _ = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "claim", id_a, "--start", "true"],
    )
    .await;
    let _ = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "complete", id_a, "--summary", "done"],
    )
    .await;

    // Verify B is unblocked
    let b_after = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "get", id_b, "--json"],
    )
    .await;
    assert!(b_after.status.success(), "get task B failed");
    let b_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&b_after.stdout)).unwrap();
    assert_eq!(
        b_json["status"].as_str(),
        Some("pending"),
        "task B must unblock when A completes"
    );

    // Re-add dependency for remove test
    let _ = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "add-dep", id_b, "--dep", id_a, "--json"],
    )
    .await;

    // Remove dependency
    let remove_out = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "remove-dep", id_b, "--dep", id_a],
    )
    .await;
    assert!(remove_out.status.success(), "remove-dep failed");

    // Verify B is unblocked after remove
    let b_removed = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        "dep-worker",
        &["task", "get", id_b, "--json"],
    )
    .await;
    assert!(b_removed.status.success(), "get task B after remove failed");
    let b_removed_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&b_removed.stdout)).unwrap();
    assert_eq!(
        b_removed_json["status"].as_str(),
        Some("pending"),
        "task B must unblock when dependency is removed"
    );
}

// ─── CLI: alias too long returns error ──────────────────────────────────────

#[tokio::test]
async fn sqlite_cli_alias_too_long_returns_error() {
    let ctx = boot_authenticated("cli-alias").await;

    let long_alias = "a".repeat(65);
    let output = run_orchy_with_flags(
        &ctx.base,
        &ctx.api_key,
        "anyproj",
        &long_alias,
        &["agent", "register", "--description", "bad"],
    )
    .await;

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "register with 65-char alias must fail"
    );
    assert!(
        stderr.contains("validation failed:"),
        "stderr must contain 'invalid input:':\n{stderr}"
    );
}
