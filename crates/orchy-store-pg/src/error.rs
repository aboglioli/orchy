use orchy_core::error::{Error as CoreError, StoreError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PgError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Domain(#[from] CoreError),
}

pub type PgResult<T> = std::result::Result<T, PgError>;

impl From<PgError> for CoreError {
    fn from(e: PgError) -> Self {
        match e {
            PgError::Domain(d) => d,
            PgError::Json(je) => CoreError::Store(StoreError::Serialization(je.to_string())),
            PgError::Sqlx(se) => CoreError::Store(categorize_sqlx(se)),
        }
    }
}

pub fn store_err(e: sqlx::Error) -> CoreError {
    PgError::Sqlx(e).into()
}

fn categorize_sqlx(e: sqlx::Error) -> StoreError {
    use sqlx::Error::*;
    match e {
        RowNotFound => StoreError::Other("sqlx returned no rows".into()),
        PoolTimedOut | PoolClosed => StoreError::PoolExhausted,
        Database(db) => {
            if db.is_unique_violation() || db.is_foreign_key_violation() || db.is_check_violation()
            {
                StoreError::Constraint(db.to_string())
            } else {
                StoreError::Other(db.to_string())
            }
        }
        Io(io) => StoreError::Connection(io.to_string()),
        Tls(t) => StoreError::Connection(t.to_string()),
        Configuration(c) => StoreError::Connection(c.to_string()),
        Migrate(m) => StoreError::Migration(m.to_string()),
        Decode(d) => StoreError::Serialization(d.to_string()),
        ColumnDecode { index, source } => StoreError::Decode {
            table: "<unknown>".to_owned(),
            column: index,
            cause: source.to_string(),
        },
        Encode(en) => StoreError::Serialization(en.to_string()),
        TypeNotFound { type_name } => StoreError::Other(format!("type not found: {type_name}")),
        WorkerCrashed => StoreError::Connection("sqlx worker crashed".into()),
        other => StoreError::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_not_found_maps_to_other() {
        let e = categorize_sqlx(sqlx::Error::RowNotFound);
        assert!(matches!(e, StoreError::Other(_)));
    }

    #[test]
    fn pool_timed_out_maps_to_pool_exhausted() {
        let e = categorize_sqlx(sqlx::Error::PoolTimedOut);
        assert!(matches!(e, StoreError::PoolExhausted));
    }

    #[test]
    fn pool_closed_maps_to_pool_exhausted() {
        let e = categorize_sqlx(sqlx::Error::PoolClosed);
        assert!(matches!(e, StoreError::PoolExhausted));
    }

    #[test]
    fn worker_crashed_maps_to_connection() {
        let e = categorize_sqlx(sqlx::Error::WorkerCrashed);
        assert!(matches!(e, StoreError::Connection(_)));
    }

    #[test]
    fn json_error_maps_to_serialization() {
        let bad: std::result::Result<serde_json::Value, _> = serde_json::from_str("{");
        let je = bad.unwrap_err();
        let pg = PgError::Json(je);
        let core: CoreError = pg.into();
        assert!(matches!(
            core,
            CoreError::Store(StoreError::Serialization(_))
        ));
    }

    #[test]
    fn domain_passes_through() {
        use orchy_core::error::DomainError;
        let domain = CoreError::Domain(DomainError::Validation("x".into()));
        let pg = PgError::Domain(domain);
        let core: CoreError = pg.into();
        assert!(matches!(
            core,
            CoreError::Domain(DomainError::Validation(_))
        ));
    }
}
