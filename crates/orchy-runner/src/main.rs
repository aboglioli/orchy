use clap::Parser;
use orchy_runner::config::{Cli, RunnerConfig};
use orchy_runner::driver::AgentDriver;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "orchy_runner=info".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = RunnerConfig::from_cli(cli);

    tracing::info!(
        alias = %config.alias,
        agent_type = %config.agent_type,
        command = %config.command,
        url = %config.url,
        project = %config.project,
        "starting orchy-runner"
    );

    if let Err(e) = AgentDriver::run(config).await {
        tracing::error!(error = %e, "agent driver exited with error");
        std::process::exit(1);
    }
}
