use std::time::Duration;
use std::{process::Stdio, string::String};

use orchy_runner::error::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

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

    println!("=== Pipe Test ===");
    println!("command: {command}");
    println!("mode: Pipe (stdin/stdout)");
    println!("json output: {is_json}");
    println!();

    let mut child = Command::new(&command)
        .args(&cmd_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            orchy_runner::error::Error::Spawn(format!("failed to spawn {command}: {e}"))
        })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| orchy_runner::error::Error::Io("child stdout not available".to_string()))?;

    println!("--- process spawned, reading output ---");
    println!();

    let read_duration = Duration::from_secs(60);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > read_duration {
            println!("\n--- timeout reached ({read_duration:?}) ---");
            break;
        }

        let mut buf = [0u8; 8192];
        let read = tokio::time::timeout(Duration::from_secs(10), stdout.read(&mut buf)).await;

        match read {
            Ok(Ok(0)) => {
                println!("\n--- process exited ---");
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
                if child
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
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&[0x03]).await;
        let _ = stdin.flush().await;
    }
    let _ = child.kill().await;

    println!("done.");
    Ok(())
}
