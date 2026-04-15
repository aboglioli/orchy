use clap::Parser;

mod agent_tab;
mod app;
mod config;
mod tui;
mod ui;

#[derive(Parser)]
#[command(name = "orchy-swarm", about = "TUI manager for AI coding agents")]
struct Cli {
    #[arg(long, env = "ORCHY_URL", default_value = "http://127.0.0.1:3100/mcp")]
    url: String,

    #[arg(long, env = "ORCHY_PROJECT")]
    project: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let project = cli.project.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "default".to_string())
    });

    let mut terminal = tui::init().expect("failed to init terminal");
    let result = app::App::new(cli.url, project).run(&mut terminal).await;
    tui::restore().expect("failed to restore terminal");

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
