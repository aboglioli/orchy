use std::fmt;
use std::result::Result as StdResult;

use thiserror::Error;

pub type Result<T> = StdResult<T, DomainError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Validation,
    InvalidTransition,
    NotFound,
    Conflict,
    Forbidden,
    UnknownType,
    UnknownRelation,
    Ambiguous,
}

impl ErrorCode {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotFound => 4,
            Self::Conflict | Self::InvalidTransition | Self::Forbidden => 5,
            Self::Validation | Self::UnknownType | Self::UnknownRelation => 6,
            Self::Ambiguous => 7,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Validation => "validation",
            Self::InvalidTransition => "invalid_transition",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Forbidden => "forbidden",
            Self::UnknownType => "unknown_type",
            Self::UnknownRelation => "unknown_relation",
            Self::Ambiguous => "ambiguous",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("{0}")]
    Validation(String),

    #[error("cannot move {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("{resource} `{id}` not found")]
    NotFound { resource: &'static str, id: String },

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("`{0}` is not a registered type")]
    UnknownType(String),

    #[error("`{0}` is not a registered relation")]
    UnknownRelation(String),

    #[error("`{input}` matches {count} entries")]
    Ambiguous { input: String, count: usize },
}

impl DomainError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn invalid_transition(from: impl fmt::Display, to: impl fmt::Display) -> Self {
        Self::InvalidTransition {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    pub fn not_found(resource: &'static str, id: impl fmt::Display) -> Self {
        Self::NotFound {
            resource,
            id: id.to_string(),
        }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Validation(_) => ErrorCode::Validation,
            Self::InvalidTransition { .. } => ErrorCode::InvalidTransition,
            Self::NotFound { .. } => ErrorCode::NotFound,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::Forbidden(_) => ErrorCode::Forbidden,
            Self::UnknownType(_) => ErrorCode::UnknownType,
            Self::UnknownRelation(_) => ErrorCode::UnknownRelation,
            Self::Ambiguous { .. } => ErrorCode::Ambiguous,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_maps_to_a_documented_exit_status() {
        let codes = [
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::InvalidTransition,
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::UnknownType,
            ErrorCode::UnknownRelation,
            ErrorCode::Ambiguous,
        ];
        for code in codes {
            assert!((4..=7).contains(&code.exit_code()), "{code} escaped 4..=7");
        }
    }

    #[test]
    fn error_reports_its_own_code() {
        assert_eq!(
            DomainError::invalid_transition("pending", "completed").code(),
            ErrorCode::InvalidTransition
        );
        assert_eq!(DomainError::not_found("task", "01J").code().exit_code(), 4);
    }
}
