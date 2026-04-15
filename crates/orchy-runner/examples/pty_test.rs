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

    // Usage: cargo run --example pty_test -- <command> [args...]
    // Examples:
    //   cargo run --example pty_test -- bash
    //   cargo run --example pty_test -- opencode run --format json "say hello in one sentence"
    //   cargo run --example pty_test -- claude -p "say hello in one sentence"
    let (command, cmd_args) = if args.len() > 1 {
        (args[1].clone(), args[2..].to_vec())
    } else {
        ("bash".to_string(), vec![])
    };

    let is_json = cmd_args.iter().any(|a| a == "json" || a.contains("json"));

    let config = AgentConfig {
        name: "test-agent".to_string(),
        command: command.clone(),
        args: cmd_args,
        spawn_mode: SpawnMode::Pty,
        env: Default::default(),
        working_dir: None,
        pty_rows: 24,
        pty_cols: 120,
    };

    println!("=== PTY Test ===");
    println!("command: {command}");
    println!("mode: PTY");
    println!("json output: {is_json}");
    println!();

    let mut process = AgentProcess::spawn(&config).await?;
    let parser = OutputParser::new(is_json);

    println!("--- process spawned, reading output ---");
    println!();

    // Read output with a timeout
    let read_duration = Duration::from_secs(30);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > read_duration {
            println!("\n--- timeout reached ({read_duration:?}) ---");
            break;
        }

        let line = tokio::time::timeout(Duration::from_secs(5), process.read_line()).await;

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
                println!("\n--- process output stream closed ---");
                break;
            }
            Ok(Err(e)) => {
                println!("[ERR] {e}");
            }
            Err(_) => {
                // 5s read timeout — check if process still alive
                if !process.is_running() {
                    println!("\n--- process exited ---");
                    break;
                }
                // still running, just no output yet
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
        }
    }

    println!("\ncleaning up...");
    let _ = process.send_ctrl_c().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = process.kill().await;

    println!("done.");
    Ok(())
}
