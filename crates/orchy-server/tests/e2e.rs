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
