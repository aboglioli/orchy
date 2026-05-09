use orchy_core::error::{Error as CoreError, StoreError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SqliteError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("mutex poisoned: {0}")]
    Poisoned(String),

    #[error(transparent)]
    Domain(#[from] CoreError),
}

pub type SqliteResult<T> = std::result::Result<T, SqliteError>;

impl<T> From<std::sync::PoisonError<T>> for SqliteError {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        SqliteError::Poisoned(e.to_string())
    }
}

impl From<SqliteError> for CoreError {
    fn from(e: SqliteError) -> Self {
        match e {
            SqliteError::Domain(d) => d,
            SqliteError::Json(je) => CoreError::Store(StoreError::Serialization(je.to_string())),
            SqliteError::Poisoned(s) => CoreError::Store(StoreError::Other(s)),
            SqliteError::Sqlite(re) => CoreError::Store(categorize_rusqlite(re)),
        }
    }
}

pub fn store_err(e: rusqlite::Error) -> CoreError {
    SqliteError::Sqlite(e).into()
}

pub fn lock_err<T>(e: std::sync::PoisonError<T>) -> CoreError {
    SqliteError::from(e).into()
}

fn categorize_rusqlite(e: rusqlite::Error) -> StoreError {
    use rusqlite::Error::*;
    use rusqlite::ffi::ErrorCode;
    match e {
        QueryReturnedNoRows => StoreError::Other("sqlite returned no rows".into()),
        SqliteFailure(ffi, msg) => match ffi.code {
            ErrorCode::ConstraintViolation => {
                StoreError::Constraint(msg.unwrap_or_else(|| "constraint violation".to_owned()))
            }
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                StoreError::Timeout(msg.unwrap_or_else(|| "database busy".to_owned()))
            }
            ErrorCode::CannotOpen
            | ErrorCode::OperationInterrupted
            | ErrorCode::SystemIoFailure => {
                StoreError::Connection(msg.unwrap_or_else(|| "i/o failure".to_owned()))
            }
            _ => StoreError::Other(format!("{ffi}: {}", msg.unwrap_or_default())),
        },
        SqlInputError { msg, .. } => StoreError::Other(format!("sql input: {msg}")),
        FromSqlConversionFailure(_, _, e) => StoreError::Decode {
            table: "<unknown>".to_owned(),
            column: "<unknown>".to_owned(),
            cause: format!("from_sql: {e}"),
        },
        ToSqlConversionFailure(e) => StoreError::Serialization(format!("to_sql: {e}")),
        InvalidColumnName(c) => StoreError::Decode {
            table: "<unknown>".to_owned(),
            column: c,
            cause: "invalid column".to_owned(),
        },
        InvalidColumnType(_, name, ty) => StoreError::Decode {
            table: "<unknown>".to_owned(),
            column: name,
            cause: format!("invalid type: {ty}"),
        },
        other => StoreError::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_returned_no_rows_maps_to_other() {
        let e = categorize_rusqlite(rusqlite::Error::QueryReturnedNoRows);
        assert!(matches!(e, StoreError::Other(_)));
    }

    #[test]
    fn invalid_column_name_maps_to_decode() {
        let e = categorize_rusqlite(rusqlite::Error::InvalidColumnName("foo".into()));
        assert!(matches!(e, StoreError::Decode { .. }));
    }

    #[test]
    fn json_error_maps_to_serialization() {
        let bad: std::result::Result<serde_json::Value, _> = serde_json::from_str("{");
        let je = bad.unwrap_err();
        let se = SqliteError::Json(je);
        let core: CoreError = se.into();
        assert!(matches!(
            core,
            CoreError::Store(StoreError::Serialization(_))
        ));
    }

    #[test]
    fn domain_passes_through() {
        use orchy_core::error::DomainError;
        let domain = CoreError::Domain(DomainError::Validation("x".into()));
        let se = SqliteError::Domain(domain);
        let core: CoreError = se.into();
        assert!(matches!(
            core,
            CoreError::Domain(DomainError::Validation(_))
        ));
    }
}
