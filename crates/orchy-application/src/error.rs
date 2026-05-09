use orchy_core::error::{DomainError, Error as CoreError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("organization mismatch")]
    OrganizationMismatch,

    #[error("embeddings provider failed: {0}")]
    EmbeddingsProvider(String),
}

pub type ApplicationResult<T> = std::result::Result<T, ApplicationError>;

impl From<DomainError> for ApplicationError {
    fn from(e: DomainError) -> Self {
        match e {
            DomainError::PasswordMismatch => {
                ApplicationError::AuthenticationFailed("invalid credentials".to_owned())
            }
            DomainError::Deactivated => {
                ApplicationError::AuthenticationFailed("user is deactivated".to_owned())
            }
            other => ApplicationError::Core(CoreError::Domain(other)),
        }
    }
}

impl From<orchy_events::Error> for ApplicationError {
    fn from(e: orchy_events::Error) -> Self {
        ApplicationError::Core(CoreError::from(e))
    }
}

impl ApplicationError {
    pub fn authentication_failed(msg: impl Into<String>) -> Self {
        Self::AuthenticationFailed(msg.into())
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    pub fn embeddings_provider(msg: impl Into<String>) -> Self {
        Self::EmbeddingsProvider(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_mismatch_maps_to_authentication_failed() {
        let e: ApplicationError = DomainError::PasswordMismatch.into();
        assert!(matches!(e, ApplicationError::AuthenticationFailed(_)));
    }

    #[test]
    fn deactivated_maps_to_authentication_failed() {
        let e: ApplicationError = DomainError::Deactivated.into();
        assert!(matches!(e, ApplicationError::AuthenticationFailed(_)));
    }

    #[test]
    fn other_domain_errors_pass_through() {
        let e: ApplicationError = DomainError::Validation("bad".into()).into();
        assert!(matches!(
            e,
            ApplicationError::Core(CoreError::Domain(DomainError::Validation(_)))
        ));
    }

    #[test]
    fn core_error_routes_through_via_from() {
        use orchy_core::error::Resource;
        let core = CoreError::not_found(Resource::Task, "t1");
        let e: ApplicationError = core.into();
        assert!(matches!(
            e,
            ApplicationError::Core(CoreError::NotFound { .. })
        ));
    }

    #[test]
    fn events_error_propagates_via_from() {
        let ev = orchy_events::Error::InvalidNamespace("bad".into());
        let e: ApplicationError = ev.into();
        assert!(matches!(
            e,
            ApplicationError::Core(CoreError::Domain(DomainError::Validation(_)))
        ));
    }
}
