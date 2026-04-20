use clap::{Parser, Subcommand};
use std::process::exit;

mod client;
mod cmd;
mod config;
mod output;

use client::OrchyClient;
use config::Config;

/// orchy — CLI for the orchy multi-agent coordination server
#[derive(Parser)]
#[command(name = "orchy")]
#[command(about = "Stateless CLI client for orchy")]
struct Cli {
    #[arg(long, help = "Server URL")]
    url: Option<String>,
    #[arg(long, help = "API key")]
    api_key: Option<String>,
    #[arg(long, help = "Organization ID")]
    org: Option<String>,
    #[arg(long, help = "Project ID")]
    project: Option<String>,
    #[arg(long, help = "Namespace")]
    namespace: Option<String>,
    #[arg(long, help = "Agent ID")]
    agent: Option<String>,
    #[arg(long, global = true, help = "Output JSON")]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Bootstrap {
        #[arg(long, help = "Show full message bodies and skill content")]
        verbose: bool,
    },
    #[command(subcommand)]
    Agent(cmd::agent::AgentSubcommand),
    #[command(subcommand)]
    Task(cmd::task::TaskSubcommand),
    #[command(subcommand)]
    Knowledge(cmd::knowledge::KnowledgeSubcommand),
    #[command(subcommand)]
    Message(cmd::message::MessageSubcommand),
    #[command(subcommand)]
    Edge(cmd::edge::EdgeSubcommand),
    #[command(subcommand)]
    Project(cmd::project::ProjectSubcommand),
    #[command(subcommand)]
    Lock(cmd::lock::LockSubcommand),
    #[command(subcommand)]
    Event(cmd::event::EventSubcommand),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config = match Config::resolve(
        cli.url.as_deref(),
        cli.api_key.as_deref(),
        cli.org.as_deref(),
        cli.project.as_deref(),
        cli.namespace.as_deref(),
        cli.agent.as_deref(),
        cli.json,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            exit(1);
        }
    };

    let client = OrchyClient::new(&config);

    let result = match cli.command {
        Commands::Bootstrap { verbose } => cmd::bootstrap::run(&client, &config, verbose).await,
        Commands::Agent(cmd) => cmd::agent::run(&cmd, &client, &config).await,
        Commands::Task(cmd) => cmd::task::run(&cmd, &client, &config).await,
        Commands::Knowledge(cmd) => cmd::knowledge::run(&cmd, &client, &config).await,
        Commands::Message(cmd) => cmd::message::run(&cmd, &client, &config).await,
        Commands::Edge(cmd) => cmd::edge::run(&cmd, &client, &config).await,
        Commands::Project(cmd) => cmd::project::run(&cmd, &client, &config).await,
        Commands::Lock(cmd) => cmd::lock::run(&cmd, &client, &config).await,
        Commands::Event(cmd) => cmd::event::run(&cmd, &client, &config).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        exit(1);
    }
}
