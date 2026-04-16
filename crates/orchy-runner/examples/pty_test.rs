use std::collections::HashMap;
use std::time::Duration;

use orchy_runner::config::RunnerConfig;
use orchy_runner::error::Result;
use orchy_runner::process::spawn_pty_raw;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    let config = RunnerConfig {
        alias: "test-agent".to_string(),
        agent_type: "unknown".to_string(),
        description: "test agent".to_string(),
        url: "http://127.0.0.1:3100/mcp".to_string(),
        project: "default".to_string(),
        namespace: None,
        command: command.clone(),
        args: cmd_args,
        env: HashMap::new(),
        working_dir: None,
        pty_rows: 24,
        pty_cols: 120,
        idle_patterns: vec![],
        idle_wake: Duration::from_secs(120),
    };

    println!("=== PTY Test ===");
    println!("command: {command}");
    println!("mode: PTY");
    println!("json output: {is_json}");
    println!();

    let mut parts = spawn_pty_raw(&config)?;

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

        let mut buf = [0u8; 8192];
        let read = tokio::time::timeout(Duration::from_secs(5), parts.reader.read(&mut buf)).await;

        match read {
            Ok(Ok(0)) => {
                println!("\n--- process output stream closed ---");
                break;
            }
            Ok(Ok(n)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                print!("{text}");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            Ok(Err(e)) => {
                println!("[ERR] {e}");
            }
            Err(_) => {
                if parts
                    .child
                    .try_wait()
                    .map_err(|e| orchy_runner::error::Error::Io(format!("wait: {e}")))?
                    .is_some()
                {
                    println!("\n--- process exited ---");
                    break;
                }
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
        }
    }

    println!("\ncleaning up...");
    let _ = parts.writer.write_all(&[0x03]).await;
    let _ = parts.writer.flush().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let _ = parts.child.kill().await;

    println!("done.");
    Ok(())
}
