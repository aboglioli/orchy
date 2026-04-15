use orchy_runner::config::RunnerConfig;
use orchy_runner::driver::AgentDriver;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "orchy_runner=info".into()),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "runner.toml".to_string());

    let config = match RunnerConfig::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to load config from {config_path}: {e}");
            std::process::exit(1);
        }
    };

    tracing::info!(
        agent = %config.agent.name,
        command = %config.agent.command,
        orchy_url = %config.orchy.url,
        project = %config.orchy.project,
        "starting orchy-runner"
    );

    if let Err(e) = AgentDriver::run(config).await {
        tracing::error!(error = %e, "agent driver exited with error");
        std::process::exit(1);
    }
}
