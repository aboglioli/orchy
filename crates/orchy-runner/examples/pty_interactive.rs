/// Interactive PTY host. Forwards stdin/stdout bit-perfect; supports optional ghost injection.
///
/// Usage:
///   cargo run --example pty_interactive -p orchy-runner -- <command> [args...]
///
/// Env vars:
///   GHOST_TEXT="say hello"   — inject this prompt after the agent starts
///   GHOST_DELAY_SECS=3       — seconds to wait before injecting (default: 3)
use std::io::{self, ErrorKind, Read};
use std::sync::Arc;
use std::time::Duration;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use orchy_runner::config::{AgentConfig, SpawnMode};
use orchy_runner::error::{Error, Result};
use orchy_runner::input::{is_focus_in_out, is_mouse_sgr_prefix, map_enter, unescape};
use orchy_runner::process::spawn_pty_raw;
use pty_process::OwnedWritePty;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, oneshot};
use tokio::time::sleep;

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        eprintln!("\r\n[pty] session end");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let (command, cmd_args) = if args.len() > 1 {
        (args[1].clone(), args[2..].to_vec())
    } else {
        ("bash".to_string(), vec![])
    };

    let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into());
    let mut env = std::collections::HashMap::new();
    env.insert("TERM".to_string(), term);

    let (rows, cols) = crossterm::terminal::size()
        .map(|(c, r)| (r, c))
        .unwrap_or((24, 80));

    let config = AgentConfig {
        name: "interactive".to_string(),
        command: command.clone(),
        args: cmd_args,
        spawn_mode: SpawnMode::Pty,
        env,
        working_dir: None,
        pty_rows: rows,
        pty_cols: cols,
        idle_patterns: vec![],
    };

    let ghost_text = std::env::var("GHOST_TEXT").ok();
    let ghost_delay = std::env::var("GHOST_DELAY_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(3);

    enable_raw_mode().map_err(|e| Error::Spawn(format!("raw mode: {e}")))?;
    let _guard = RawModeGuard;

    let parts = spawn_pty_raw(&config)?;
    let writer: Arc<Mutex<OwnedWritePty>> = Arc::new(Mutex::new(parts.writer));
    let mut child = parts.child;

    spawn_pty_to_stdout(parts.reader);

    if let Some(text) = ghost_text {
        spawn_ghost(text, ghost_delay, Arc::clone(&writer));
    }

    let stdin_done = spawn_stdin_to_pty(Arc::clone(&writer));

    tokio::select! {
        _ = stdin_done => { let _ = child.kill().await; }
        _ = child.wait() => {}
    }

    Ok(())
}

fn spawn_pty_to_stdout(mut reader: pty_process::OwnedReadPty) {
    tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    let _ = out.flush().await;
                }
            }
        }
    });
}

fn spawn_ghost(text: String, delay_secs: u64, writer: Arc<Mutex<OwnedWritePty>>) {
    tokio::spawn(async move {
        sleep(Duration::from_secs(delay_secs)).await;
        let body = unescape(&text).into_bytes();
        let mut w = writer.lock().await;
        if w.write_all(&body).await.is_err() {
            return;
        }
        let _ = w.flush().await;
        drop(w);
        sleep(Duration::from_millis(350)).await;
        let mut w = writer.lock().await;
        let _ = w.write_all(b"\r").await;
        let _ = w.flush().await;
    });
}

fn spawn_stdin_to_pty(writer: Arc<Mutex<OwnedWritePty>>) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    let rt = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let raw = &buf[..n];
                    if raw.contains(&0x03) {
                        break;
                    }
                    if is_mouse_sgr_prefix(raw) || is_focus_in_out(raw) {
                        continue;
                    }
                    let processed = map_enter(raw);
                    let w = Arc::clone(&writer);
                    if rt
                        .block_on(async move {
                            let mut g = w.lock().await;
                            g.write_all(&processed).await?;
                            g.flush().await
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = tx.send(());
    });
    rx
}
