pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("spawn error: {0}")]
    Spawn(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("agent process exited unexpectedly")]
    ProcessExited,
}
