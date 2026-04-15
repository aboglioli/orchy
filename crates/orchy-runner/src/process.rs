use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;

use crate::config::{AgentConfig, SpawnMode};
use crate::error::{Error, Result};

pub struct PtyParts {
    pub reader: pty_process::OwnedReadPty,
    pub writer: pty_process::OwnedWritePty,
    pub child: Child,
}

pub fn spawn_pty_raw(config: &AgentConfig) -> Result<PtyParts> {
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

enum ProcessInner {
    Pty {
        writer: pty_process::OwnedWritePty,
        reader: BufReader<pty_process::OwnedReadPty>,
        child: Child,
    },
    Pipe {
        stdin: tokio::process::ChildStdin,
        reader: BufReader<tokio::process::ChildStdout>,
        child: Child,
    },
}

pub struct AgentProcess {
    inner: ProcessInner,
    name: String,
}

impl AgentProcess {
    pub async fn spawn(config: &AgentConfig) -> Result<Self> {
        let inner = match config.spawn_mode {
            SpawnMode::Pty => Self::spawn_pty(config)?,
            SpawnMode::Pipe => Self::spawn_pipe(config)?,
        };

        Ok(Self {
            inner,
            name: config.name.clone(),
        })
    }

    fn spawn_pty(config: &AgentConfig) -> Result<ProcessInner> {
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

        tracing::info!(agent = %config.name, mode = "pty", "spawned agent process");

        Ok(ProcessInner::Pty {
            writer,
            reader: BufReader::new(reader),
            child,
        })
    }

    fn spawn_pipe(config: &AgentConfig) -> Result<ProcessInner> {
        let mut cmd = tokio::process::Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        for (key, val) in &config.env {
            cmd.env(key, val);
        }

        if let Some(dir) = &config.working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| Error::Spawn(format!("failed to spawn {}: {e}", config.command)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Spawn("failed to capture stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Spawn("failed to capture stdout".into()))?;

        tracing::info!(agent = %config.name, mode = "pipe", "spawned agent process");

        Ok(ProcessInner::Pipe {
            stdin,
            reader: BufReader::new(stdout),
            child,
        })
    }

    pub async fn write(&mut self, input: &str) -> Result<()> {
        match &mut self.inner {
            ProcessInner::Pty { writer, .. } => {
                AsyncWriteExt::write_all(writer, input.as_bytes())
                    .await
                    .map_err(|e| Error::Io(format!("write to PTY: {e}")))?;
                AsyncWriteExt::flush(writer)
                    .await
                    .map_err(|e| Error::Io(format!("flush PTY: {e}")))?;
            }
            ProcessInner::Pipe { stdin, .. } => {
                AsyncWriteExt::write_all(stdin, input.as_bytes())
                    .await
                    .map_err(|e| Error::Io(format!("write to stdin: {e}")))?;
                AsyncWriteExt::flush(stdin)
                    .await
                    .map_err(|e| Error::Io(format!("flush stdin: {e}")))?;
            }
        }

        tracing::trace!(agent = %self.name, bytes = input.len(), "wrote to process");
        Ok(())
    }

    pub async fn write_line(&mut self, input: &str) -> Result<()> {
        self.write(&format!("{input}\n")).await
    }

    pub async fn read_line(&mut self) -> Result<Option<String>> {
        let mut line = String::new();

        let bytes_read = match &mut self.inner {
            ProcessInner::Pty { reader, .. } => AsyncBufReadExt::read_line(reader, &mut line)
                .await
                .map_err(|e| Error::Io(format!("read from PTY: {e}")))?,
            ProcessInner::Pipe { reader, .. } => AsyncBufReadExt::read_line(reader, &mut line)
                .await
                .map_err(|e| Error::Io(format!("read from stdout: {e}")))?,
        };

        if bytes_read == 0 {
            return Ok(None);
        }

        Ok(Some(line))
    }

    pub async fn send_ctrl_c(&mut self) -> Result<()> {
        self.write("\x03").await
    }

    pub async fn send_ctrl_d(&mut self) -> Result<()> {
        self.write("\x04").await
    }

    pub async fn kill(&mut self) -> Result<()> {
        match &mut self.inner {
            ProcessInner::Pty { child, .. } | ProcessInner::Pipe { child, .. } => {
                child
                    .kill()
                    .await
                    .map_err(|e| Error::Io(format!("kill process: {e}")))?;
            }
        }

        tracing::info!(agent = %self.name, "killed agent process");
        Ok(())
    }

    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        match &mut self.inner {
            ProcessInner::Pty { child, .. } | ProcessInner::Pipe { child, .. } => child
                .try_wait()
                .map_err(|e| Error::Io(format!("try_wait: {e}"))),
        }
    }

    pub fn is_running(&mut self) -> bool {
        self.try_wait().ok().flatten().is_none()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
