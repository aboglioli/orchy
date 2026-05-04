use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use orchy_application::{ApiKeyPrincipal, HeartbeatCommand, ResolveApiKeyCommand};
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};
use tokio::net::TcpListener;
use tower::Service;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use orchy_server::api;
use orchy_server::bootstrap;
use orchy_server::config::Config;
use orchy_server::container::Container;
use orchy_server::error::{BootError, BootResult};
use orchy_server::heartbeat::run_heartbeat_monitor;
use orchy_server::mcp::OrchyHandler;
use orchy_server::skill_loader;
use tower_cookies::CookieManagerLayer;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        error!(error = %e, "orchy server failed");
        std::process::exit(1);
    }
}

async fn run() -> BootResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(
                "orchy=info"
                    .parse()
                    .map_err(|e| BootError::Config(format!("invalid log directive: {e}")))?,
            ),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config_content = std::fs::read_to_string(&config_path)
        .map_err(|e| BootError::Config(format!("failed to read config file {config_path}: {e}")))?;

    let config: Config = toml::from_str(&config_content)
        .map_err(|e| BootError::Config(format!("failed to parse config file: {e}")))?;

    config
        .validate()
        .map_err(|e| BootError::Config(format!("invalid config: {e}")))?;

    let host = config.server.host.clone();
    let port = config.server.port;

    let container = Container::from_config(config).await?;

    if let Some(ref skills_config) = container.config.skills {
        let dir = std::path::Path::new(&skills_config.dir);
        skill_loader::load_skills_from_dir(dir, &container.app)
            .await
            .map_err(|e| BootError::Other(format!("failed to load skills from disk: {e}")))?;
    }

    let heartbeat_container = Arc::clone(&container);
    tokio::spawn(async move {
        run_heartbeat_monitor(heartbeat_container).await;
    });

    let session_pruner_container = Arc::clone(&container);
    tokio::spawn(async move {
        run_session_pruner(session_pruner_container).await;
    });

    let bootstrap_container = Arc::clone(&container);
    let _mcp_container = Arc::clone(&container);

    let mut session_manager = LocalSessionManager::default();
    session_manager.session_config.keep_alive = container.config.server.mcp_session_keep_alive();
    let session_manager = Arc::new(session_manager);

    let router = axum::Router::new()
        .route(
            "/mcp",
            axum::routing::post(move |headers, state, request: axum::extract::Request| {
                mcp_handler(headers, state, request, Arc::clone(&session_manager))
            }),
        )
        .route(
            "/bootstrap/{namespace}",
            axum::routing::get(bootstrap_handler),
        )
        .nest("/api", api::router())
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&bootstrap_container),
            heartbeat_middleware,
        ))
        .layer(CookieManagerLayer::new())
        .with_state(bootstrap_container);

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| BootError::Other(format!("failed to bind to {addr}: {e}")))?;

    info!(%addr, "orchy server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| BootError::Other(format!("server error: {e}")))?;

    info!("orchy server shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("SIGINT received, beginning graceful shutdown"),
        _ = terminate => info!("SIGTERM received, beginning graceful shutdown"),
    }
}

async fn mcp_handler(
    headers: axum::http::HeaderMap,
    State(container): State<Arc<Container>>,
    request: axum::extract::Request,
    session_manager: Arc<LocalSessionManager>,
) -> axum::response::Response {
    let auth = match resolve_mcp_auth(&headers, &container.app).await {
        Ok(auth) => auth,
        Err(e) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                format!("Unauthorized: {e}"),
            )
                .into_response();
        }
    };

    let container_clone = Arc::clone(&container);
    let mut service = StreamableHttpService::new(
        move || {
            OrchyHandler::new(container_clone.clone(), auth.clone()).map_err(std::io::Error::other)
        },
        session_manager,
        Default::default(),
    );

    match service.call(request).await {
        Ok(response) => response.into_response(),
        Err(e) => {
            warn!(error = %e, "MCP service error");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Internal error",
            )
                .into_response()
        }
    }
}

async fn resolve_mcp_auth(
    headers: &axum::http::HeaderMap,
    app: &orchy_application::Application,
) -> Result<ApiKeyPrincipal, String> {
    let key = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or("missing or invalid Authorization header")?;

    let principal = app
        .resolve_api_key
        .execute(ResolveApiKeyCommand {
            raw_key: key.to_string(),
        })
        .await
        .map_err(|e| format!("API key resolution failed: {e}"))?
        .ok_or("invalid or expired API key")?;

    Ok(principal)
}

async fn heartbeat_middleware(
    State(container): State<Arc<Container>>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use std::collections::HashMap;

    let query: axum::extract::Query<HashMap<String, String>> =
        axum::extract::Query::try_from_uri(req.uri()).unwrap_or_default();

    if let Some(session_id) = query.get("sessionId") {
        let session_agents = container.session_agents.read().await;
        if let Some(agent_id) = session_agents.get(session_id) {
            let container = Arc::clone(&container);
            let agent_id = agent_id.to_string();
            tokio::spawn(async move {
                let cmd = HeartbeatCommand { agent_id };
                let _ = container.app.heartbeat.execute(cmd).await;
            });
        }
    }

    next.run(req).await
}

async fn bootstrap_handler(
    State(container): State<Arc<Container>>,
    Path(namespace): Path<String>,
) -> impl IntoResponse {
    let (project_str, scope) = match namespace.split_once('/') {
        Some((p, s)) => (p.to_string(), Some(s.to_string())),
        None => (namespace.clone(), None),
    };

    let project_id = match orchy_core::namespace::ProjectId::try_from(project_str) {
        Ok(p) => p,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    };

    let ns = match scope {
        Some(s) => match orchy_core::namespace::Namespace::try_from(format!("/{s}")) {
            Ok(ns) => ns,
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        },
        None => orchy_core::namespace::Namespace::root(),
    };

    let host = &container.config.server.host;
    let port = container.config.server.port;

    match bootstrap::generate_bootstrap_prompt(&project_id, &ns, host, port, &container.app).await {
        Ok(prompt) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            prompt,
        )
            .into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
    }
}

async fn run_session_pruner(container: Arc<Container>) {
    use std::collections::HashSet;
    use tokio::time::{Duration, sleep};

    let interval = Duration::from_secs(300);

    loop {
        sleep(interval).await;

        let snapshot: Vec<(String, orchy_core::agent::AgentId)> = {
            let sessions = container.session_agents.read().await;
            sessions
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        if snapshot.is_empty() {
            continue;
        }

        let agent_ids: Vec<orchy_core::agent::AgentId> =
            snapshot.iter().map(|(_, id)| id.clone()).collect();
        let alive: HashSet<orchy_core::agent::AgentId> = match container
            .agents
            .find_by_ids(&agent_ids)
            .await
        {
            Ok(found) => found.into_iter().map(|a| a.id().clone()).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "session pruner: failed to query agents, skipping cycle");
                continue;
            }
        };

        let mut pruned = 0u32;
        let mut sessions = container.session_agents.write().await;
        sessions.retain(|_, agent_id| {
            let keep = alive.contains(agent_id);
            if !keep {
                pruned += 1;
            }
            keep
        });
        if pruned > 0 {
            tracing::info!(pruned, "session pruner removed stale sessions");
        }
    }
}
