use std::sync::Arc;

use rmcp::transport::{
    streamable_http_server::session::local::LocalSessionManager, StreamableHttpService,
};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

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

    let config: Config =
        toml::from_str(&config_content).expect("failed to parse config file");

    let host = config.server.host.clone();
    let port = config.server.port;

    let container = Container::from_config(config)
        .await
        .expect("failed to build container");

    if let Some(ref skills_config) = container.config.skills {
        let dir = std::path::Path::new(&skills_config.dir);
        skill_loader::load_skills_from_dir(dir, &container.skill_service)
            .await
            .expect("failed to load skills from disk");
    }

    let heartbeat_container = Arc::clone(&container);
    tokio::spawn(async move {
        run_heartbeat_monitor(heartbeat_container).await;
    });
    let service = StreamableHttpService::new(
        move || Ok(OrchyHandler::new(container.clone())),
        Arc::new(LocalSessionManager::default()),
        Default::default(),
    );

    let router = axum::Router::new().nest_service("/mcp", service);

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"));

    info!(%addr, "orchy server listening");

    axum::serve(listener, router)
        .await
        .expect("server error");
}
