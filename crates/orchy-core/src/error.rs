use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Agent,
    ApiKey,
    Edge,
    Knowledge,
    Lock,
    Message,
    Namespace,
    Organization,
    Project,
    Task,
    User,
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Resource::Agent => "agent",
            Resource::ApiKey => "api_key",
            Resource::Edge => "edge",
            Resource::Knowledge => "knowledge",
            Resource::Lock => "lock",
            Resource::Message => "message",
            Resource::Namespace => "namespace",
            Resource::Organization => "organization",
            Resource::Project => "project",
            Resource::Task => "task",
            Resource::User => "user",
        };
        f.write_str(s)
    }
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("connection failed: {0}")]
    Connection(String),

    #[error("connection pool exhausted")]
    PoolExhausted,

    #[error("operation timed out: {0}")]
    Timeout(String),

    #[error("constraint violation: {0}")]
    Constraint(String),

    #[error("row not found")]
    NotFound,

    #[error("decode {table}.{column}: {cause}")]
    Decode {
        table: String,
        column: String,
        cause: String,
    },

    #[error("serialization failed: {0}")]
    Serialization(String),

    #[error("migration failed: {0}")]
    Migration(String),

    #[error("store error: {0}")]
    Other(String),
}

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("validation failed: {0}")]
    Validation(String),

    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("dependency not met: task {0} is not completed")]
    DependencyNotMet(String),

    #[error("rule violated: {0}")]
    RuleViolation(String),

    #[error("password mismatch")]
    PasswordMismatch,

    #[error("entity deactivated")]
    Deactivated,

    #[error("internal: {0}")]
    Internal(String),
}

pub type DomainResult<T> = std::result::Result<T, DomainError>;

impl DomainError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn rule_violation(msg: impl Into<String>) -> Self {
        Self::RuleViolation(msg.into())
    }

    pub fn invalid_transition(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::InvalidTransition {
            from: from.into(),
            to: to.into(),
        }
    }

    pub fn dependency_not_met(task_id: impl Into<String>) -> Self {
        Self::DependencyNotMet(task_id.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl From<orchy_events::Error> for DomainError {
    fn from(e: orchy_events::Error) -> Self {
        use orchy_events::Error as Ev;
        match e {
            Ev::InvalidTopic(s)
            | Ev::InvalidNamespace(s)
            | Ev::InvalidOrganization(s)
            | Ev::InvalidMetadataKey(s)
            | Ev::InvalidPayload(s)
            | Ev::InvalidEventKey(s)
            | Ev::InvalidConsumerGroupId(s)
            | Ev::InvalidStartFrom(s)
            | Ev::Config(s) => DomainError::Validation(s),
            Ev::Serialization(s) | Ev::Store(s) | Ev::Timeout(s) => DomainError::Internal(s),
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("{resource} not found: {id}")]
    NotFound { resource: Resource, id: String },

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u64, actual: u64 },

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    Domain(#[from] DomainError),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn not_found(resource: Resource, id: impl Into<String>) -> Self {
        Self::NotFound {
            resource,
            id: id.into(),
        }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::Domain(DomainError::Validation(msg.into()))
    }

    pub fn invalid_transition(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::Domain(DomainError::invalid_transition(from, to))
    }

    pub fn dependency_not_met(task_id: impl Into<String>) -> Self {
        Self::Domain(DomainError::dependency_not_met(task_id.into()))
    }

    pub fn version_mismatch(expected: u64, actual: u64) -> Self {
        Self::VersionMismatch { expected, actual }
    }
}

impl From<crate::resource_ref::ResourceKind> for Resource {
    fn from(k: crate::resource_ref::ResourceKind) -> Self {
        use crate::resource_ref::ResourceKind;
        match k {
            ResourceKind::Task => Resource::Task,
            ResourceKind::Knowledge => Resource::Knowledge,
            ResourceKind::Agent => Resource::Agent,
            ResourceKind::Message => Resource::Message,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Store(StoreError::Serialization(e.to_string()))
    }
}

impl From<orchy_events::Error> for Error {
    fn from(e: orchy_events::Error) -> Self {
        Error::Domain(DomainError::from(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_invalid_variants_map_to_domain_validation() {
        let cases = [
            orchy_events::Error::InvalidTopic("t".into()),
            orchy_events::Error::InvalidNamespace("n".into()),
            orchy_events::Error::InvalidOrganization("o".into()),
            orchy_events::Error::InvalidMetadataKey("m".into()),
            orchy_events::Error::InvalidPayload("p".into()),
            orchy_events::Error::InvalidEventKey("k".into()),
            orchy_events::Error::InvalidConsumerGroupId("c".into()),
            orchy_events::Error::InvalidStartFrom("s".into()),
            orchy_events::Error::Config("c".into()),
        ];
        for ev in cases {
            let de: DomainError = ev.into();
            assert!(matches!(de, DomainError::Validation(_)));
        }
    }

    #[test]
    fn events_infra_variants_map_to_domain_internal() {
        let cases = [
            orchy_events::Error::Serialization("s".into()),
            orchy_events::Error::Store("s".into()),
            orchy_events::Error::Timeout("t".into()),
        ];
        for ev in cases {
            let de: DomainError = ev.into();
            assert!(matches!(de, DomainError::Internal(_)));
        }
    }

    #[test]
    fn events_error_routes_through_to_domain_in_error() {
        let ev = orchy_events::Error::InvalidTopic("bad".into());
        let e: Error = ev.into();
        assert!(matches!(e, Error::Domain(DomainError::Validation(_))));
    }

    #[test]
    fn serde_json_error_maps_to_store_serialization() {
        let bad: std::result::Result<serde_json::Value, _> = serde_json::from_str("{not json");
        let je = bad.unwrap_err();
        let e: Error = je.into();
        assert!(matches!(e, Error::Store(StoreError::Serialization(_))));
    }

    #[test]
    fn helper_constructors_route_via_domain() {
        let e = Error::invalid_input("x");
        assert!(matches!(e, Error::Domain(DomainError::Validation(_))));

        let e = Error::invalid_transition("from", "to");
        assert!(matches!(
            e,
            Error::Domain(DomainError::InvalidTransition { .. })
        ));

        let e = Error::dependency_not_met("task1");
        assert!(matches!(e, Error::Domain(DomainError::DependencyNotMet(_))));

        let e = Error::version_mismatch(1, 2);
        assert!(matches!(
            e,
            Error::VersionMismatch {
                expected: 1,
                actual: 2
            }
        ));
    }

    #[test]
    fn not_found_carries_resource_and_id() {
        let e = Error::not_found(Resource::Task, "task-123");
        match e {
            Error::NotFound { resource, id } => {
                assert_eq!(resource, Resource::Task);
                assert_eq!(id, "task-123");
            }
            _ => panic!("expected NotFound"),
        }
    }

    #[test]
    fn resource_kind_converts_to_resource() {
        use crate::resource_ref::ResourceKind;
        assert_eq!(Resource::from(ResourceKind::Task), Resource::Task);
        assert_eq!(Resource::from(ResourceKind::Knowledge), Resource::Knowledge);
        assert_eq!(Resource::from(ResourceKind::Agent), Resource::Agent);
        assert_eq!(Resource::from(ResourceKind::Message), Resource::Message);
    }

    #[test]
    fn store_error_decode_displays_table_column() {
        let e = StoreError::Decode {
            table: "users".into(),
            column: "email".into(),
            cause: "bad utf8".into(),
        };
        assert_eq!(e.to_string(), "decode users.email: bad utf8");
    }

    #[test]
    fn resource_display() {
        assert_eq!(Resource::Task.to_string(), "task");
        assert_eq!(Resource::ApiKey.to_string(), "api_key");
    }
}
