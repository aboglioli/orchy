use crate::config::RunnerConfig;
use crate::error::{Error, Result};

pub struct PtyParts {
    pub reader: pty_process::OwnedReadPty,
    pub writer: pty_process::OwnedWritePty,
    pub child: tokio::process::Child,
}

pub fn spawn_pty_raw(config: &RunnerConfig) -> Result<PtyParts> {
    let (pty, pts) =
        pty_process::open().map_err(|e| Error::Spawn(format!("failed to open PTY: {e}")))?;

    pty.resize(pty_process::Size::new(config.pty_rows, config.pty_cols))
        .map_err(|e| Error::Spawn(format!("failed to resize PTY: {e}")))?;

    let mut cmd = pty_process::Command::new(&config.command).args(&config.args);

    for (key, val) in &config.env {
        cmd = cmd.env(key, val);
    }

    if let Some(dir) = &config.working_dir {
        cmd = cmd.current_dir(dir);
    }

    let child = cmd
        .spawn(pts)
        .map_err(|e| Error::Spawn(format!("failed to spawn {}: {e}", config.command)))?;

    let (reader, writer) = pty.into_split();

    Ok(PtyParts {
        reader,
        writer,
        child,
    })
}
