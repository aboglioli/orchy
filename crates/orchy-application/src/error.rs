use std::fmt;
use std::result::Result as StdResult;

use orchy_core::{DomainError, ErrorCode};
use thiserror::Error;

pub type ApplicationResult<T> = StdResult<T, ApplicationError>;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
}

impl ApplicationError {
    pub fn not_found(resource: &'static str, id: impl fmt::Display) -> Self {
        Self::Domain(DomainError::not_found(resource, id))
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Domain(e) => e.code(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.code().exit_code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_domain_error_keeps_its_own_code_through_the_application_boundary() {
        let err: ApplicationError = DomainError::not_found("task", "01ARZ").into();
        assert_eq!(err.code(), ErrorCode::NotFound);
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn a_storage_failure_exits_distinctly_from_any_domain_refusal() {
        let err: ApplicationError = DomainError::unavailable("disk gone").into();
        assert_eq!(err.exit_code(), 8);
        let refused: ApplicationError = DomainError::conflict("held").into();
        assert_ne!(err.exit_code(), refused.exit_code());
    }
}
