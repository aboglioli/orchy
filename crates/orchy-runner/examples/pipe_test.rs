use std::time::Duration;

use orchy_runner::config::{AgentConfig, SpawnMode};
use orchy_runner::error::Result;
use orchy_runner::output::OutputParser;
use orchy_runner::process::AgentProcess;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("orchy_runner=debug")
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Usage: cargo run --example pipe_test -- <command> [args...]
    // Examples:
    //   cargo run --example pipe_test -- opencode run --format json "say hello in one sentence"
    //   cargo run --example pipe_test -- claude -p --output-format stream-json "say hello"
    let (command, cmd_args) = if args.len() > 1 {
        (args[1].clone(), args[2..].to_vec())
    } else {
        eprintln!("usage: cargo run --example pipe_test -- <command> [args...]");
        std::process::exit(1);
    };

    let is_json = cmd_args.iter().any(|a| a == "json" || a.contains("json"));

    let config = AgentConfig {
        name: "test-agent".to_string(),
        command: command.clone(),
        args: cmd_args,
        spawn_mode: SpawnMode::Pipe,
        env: Default::default(),
        working_dir: None,
        pty_rows: 24,
        pty_cols: 120,
    };

    println!("=== Pipe Test ===");
    println!("command: {command}");
    println!("mode: Pipe (stdin/stdout)");
    println!("json output: {is_json}");
    println!();

    let mut process = AgentProcess::spawn(&config).await?;
    let parser = OutputParser::new(is_json);

    println!("--- process spawned, reading output ---");
    println!();

    let read_duration = Duration::from_secs(60);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > read_duration {
            println!("\n--- timeout reached ({read_duration:?}) ---");
            break;
        }

        let line = tokio::time::timeout(Duration::from_secs(10), process.read_line()).await;

        match line {
            Ok(Ok(Some(raw))) => {
                let parsed = parser.parse(&raw);

                if let Some(text) = parser.extract_text(&parsed) {
                    println!("[OUT] {text}");
                }

                if parser.is_completion_signal(&parsed) {
                    println!("\n--- completion signal detected ---");
                    break;
                }
            }
            Ok(Ok(None)) => {
                println!("\n--- process exited ---");
                break;
            }
            Ok(Err(e)) => {
                println!("[ERR] {e}");
            }
            Err(_) => {
                if !process.is_running() {
                    println!("\n--- process exited ---");
                    break;
                }
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
        }
    }

    println!("\ncleaning up...");
    let _ = process.kill().await;

    println!("done.");
    Ok(())
}
