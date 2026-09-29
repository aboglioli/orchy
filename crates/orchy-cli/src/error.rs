use std::fmt;

use orchy_application::ApplicationError;
use orchy_core::DomainError;

pub(crate) type CliResult<T> = Result<T, CliError>;

#[derive(Debug)]
pub(crate) enum CliError {
    Application(ApplicationError),
    Config(String),
    Io(std::io::Error),
    NotAVault(String),
    WrongEntity(String),
    ProblemsRemain(usize),
}

impl CliError {
    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    pub(crate) fn io(e: std::io::Error) -> Self {
        Self::Io(e)
    }

    pub(crate) fn skill_given_to_a_document_command(id: impl fmt::Display) -> Self {
        Self::WrongEntity(format!(
            "`{id}` is a skill, not a document: use `orchy skill show|write|set|retire`"
        ))
    }

    pub(crate) fn not_a_vault(path: impl fmt::Display) -> Self {
        Self::NotAVault(format!(
            "{path} is not an orchy vault. Run `orchy init {path}` to create one."
        ))
    }

    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Self::Application(e) => e.exit_code(),
            Self::Config(_) => 6,
            Self::NotAVault(_) | Self::WrongEntity(_) => 4,
            Self::ProblemsRemain(_) => 6,
            Self::Io(_) => 8,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(e) => write!(f, "{e}"),
            Self::Config(m) | Self::NotAVault(m) | Self::WrongEntity(m) => f.write_str(m),
            Self::ProblemsRemain(n) => write!(
                f,
                "{n} problem{} left; `orchy doctor --fix` repairs what needs no decision",
                if *n == 1 { "" } else { "s" }
            ),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<ApplicationError> for CliError {
    fn from(e: ApplicationError) -> Self {
        Self::Application(e)
    }
}

impl From<DomainError> for CliError {
    fn from(e: DomainError) -> Self {
        Self::Application(ApplicationError::Domain(e))
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
