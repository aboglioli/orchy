use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use rmcp::transport::{
    StreamableHttpService, streamable_http_server::session::local::LocalSessionManager,
};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

use orchy_server::bootstrap;
use orchy_server::config::Config;
use orchy_server::container::Container;
use orchy_server::heartbeat::run_heartbeat_monitor;
use orchy_server::mcp::OrchyHandler;
use orchy_server::skill_loader;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("orchy=info".parse().unwrap()))
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config_content = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("failed to read config file {config_path}: {e}"));

    let config: Config = toml::from_str(&config_content).expect("failed to parse config file");

    let host = config.server.host.clone();
    let port = config.server.port;

    let container = Container::from_config(config)
        .await
        .expect("failed to build container");

    if let Some(ref skills_config) = container.config.skills {
        let dir = std::path::Path::new(&skills_config.dir);
        skill_loader::load_skills_from_dir(dir, &container.knowledge_service)
            .await
            .expect("failed to load skills from disk");
    }

    let heartbeat_container = Arc::clone(&container);
    tokio::spawn(async move {
        run_heartbeat_monitor(heartbeat_container).await;
    });

    let bootstrap_container = Arc::clone(&container);
    let mcp_container = container;

    let mut session_manager = LocalSessionManager::default();
    session_manager.session_config.keep_alive = None;

    let service = StreamableHttpService::new(
        move || Ok(OrchyHandler::new(mcp_container.clone())),
        Arc::new(session_manager),
        Default::default(),
    );

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .route(
            "/bootstrap/{namespace}",
            axum::routing::get(bootstrap_handler),
        )
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&bootstrap_container),
            heartbeat_middleware,
        ))
        .with_state(bootstrap_container);

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"));

    info!(%addr, "orchy server listening");

    axum::serve(listener, router).await.expect("server error");
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
            let agent_id = agent_id.clone();
            tokio::spawn(async move {
                let _ = container.agent_service.heartbeat(&agent_id).await;
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
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
        },
        None => orchy_core::namespace::Namespace::root(),
    };

    let host = &container.config.server.host;
    let port = container.config.server.port;

    match bootstrap::generate_bootstrap_prompt(
        &project_id,
        &ns,
        host,
        port,
        &container.knowledge_service,
        &container.project_service,
        &container.agent_service,
        &container.task_service,
    )
    .await
    {
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
