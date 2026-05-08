use std::str::FromStr;

use async_trait::async_trait;
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::user::{Email, HashedPassword, RestoreUser, User, UserId, UserStore};

use crate::{SqliteConn, blocking, blocking_tx, events};

type UserRow = (String, String, String, i32, i32, String, String);

fn build_user(row: UserRow) -> Result<User> {
    let (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at) = row;
    let id = UserId::from_str(&id).map_err(|e| {
        Error::Store(StoreError::Decode {
            table: "users".to_string(),
            column: "user_id".to_string(),
            cause: e.to_string(),
        })
    })?;
    let email = Email::new(&email).map_err(|e| {
        Error::Store(StoreError::Decode {
            table: "users".to_string(),
            column: "email".to_string(),
            cause: e.to_string(),
        })
    })?;
    let password_hash = HashedPassword::new(&password_hash).map_err(|e| {
        Error::Store(StoreError::Other(format!(
            "invalid password hash in db: {e}"
        )))
    })?;
    let created_at = created_at.parse().map_err(|e: chrono::ParseError| {
        Error::Store(StoreError::Decode {
            table: "users".to_string(),
            column: "created_at".to_string(),
            cause: e.to_string(),
        })
    })?;
    let updated_at = updated_at.parse().map_err(|e: chrono::ParseError| {
        Error::Store(StoreError::Decode {
            table: "users".to_string(),
            column: "updated_at".to_string(),
            cause: e.to_string(),
        })
    })?;
    Ok(User::restore(RestoreUser {
        id,
        email,
        password_hash,
        is_active: is_active != 0,
        is_platform_admin: is_platform_admin != 0,
        created_at,
        updated_at,
    }))
}

pub struct SqliteUserStore {
    conn: SqliteConn,
}

impl SqliteUserStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl UserStore for SqliteUserStore {
    async fn save(&self, user: &mut User) -> Result<()> {
        let id = user.id().to_string();
        let email = user.email().as_str().to_string();
        let password_hash = user.password_hash().as_str().to_string();
        let is_active = user.is_active() as i32;
        let is_platform_admin = user.is_platform_admin() as i32;
        let created_at = user.created_at().to_rfc3339();
        let updated_at = user.updated_at().to_rfc3339();
        let drained = user.drain_events();
        blocking_tx(&self.conn, move |tx| {
            tx.execute(
                "INSERT INTO users (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    email = excluded.email,
                    password_hash = excluded.password_hash,
                    is_active = excluded.is_active,
                    is_platform_admin = excluded.is_platform_admin,
                    updated_at = excluded.updated_at",
                rusqlite::params![
                    id, email, password_hash, is_active, is_platform_admin, created_at, updated_at,
                ],
            )
            .map_err(crate::error::store_err)?;
            events::write_events_in_tx(tx, &drained)?;
            Ok(())
        })
        .await
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let id = id.to_string();
        let row = blocking(&self.conn, move |conn| {
            conn.query_row(
                "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at
                 FROM users WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i32>(3)?,
                        row.get::<_, i32>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(crate::error::store_err)
        })
        .await?;

        row.map(build_user).transpose()
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>> {
        let email = email.as_str().to_string();
        let row = blocking(&self.conn, move |conn| {
            conn.query_row(
                "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at
                 FROM users WHERE email = ?1",
                rusqlite::params![email],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i32>(3)?,
                        row.get::<_, i32>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(crate::error::store_err)
        })
        .await?;

        row.map(build_user).transpose()
    }

    async fn list_all(&self) -> Result<Vec<User>> {
        let rows = blocking(&self.conn, move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at
                     FROM users ORDER BY created_at DESC"
                )
                .map_err(crate::error::store_err)?;

            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(crate::error::store_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(crate::error::store_err)
        })
        .await?;

        rows.into_iter().map(build_user).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orchy_core::error::DomainResult;
    use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};
    use orchy_core::user::{
        Email, HashedPassword, OrgMembership, OrgMembershipStore, OrgRole, PasswordHasher,
        PlainPassword, User, UserId, UserStore,
    };

    use crate::{
        SqliteDatabase, SqliteOrgMembershipStore, SqliteOrganizationStore, SqliteUserStore,
    };

    struct NoopHasher;

    impl PasswordHasher for NoopHasher {
        fn hash(&self, plain: &PlainPassword) -> DomainResult<HashedPassword> {
            HashedPassword::new(plain.as_str())
        }

        fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> DomainResult<()> {
            if plain.as_str() == hashed.as_str() {
                return Ok(());
            }

            Err(orchy_core::error::DomainError::PasswordMismatch)
        }
    }

    #[tokio::test]
    async fn saving_existing_user_preserves_memberships() {
        let db = SqliteDatabase::new(":memory:", None).unwrap();
        db.run_migrations(&SqliteDatabase::migrations_dir())
            .unwrap();
        let conn = db.conn();
        let users = SqliteUserStore::new(Arc::clone(&conn));
        let orgs = SqliteOrganizationStore::new(Arc::clone(&conn));
        let memberships = SqliteOrgMembershipStore::new(conn);
        let hasher = NoopHasher;

        let org_id = OrganizationId::new("default").unwrap();
        let mut org = Organization::new(org_id.clone(), "Default".to_string()).unwrap();
        orgs.save(&mut org).await.unwrap();

        let password = PlainPassword::new("12345678").unwrap();
        let mut user = User::register(
            UserId::new(),
            Email::new("agent@example.com").unwrap(),
            &password,
            &hasher,
        )
        .unwrap();
        users.save(&mut user).await.unwrap();

        let mut membership = OrgMembership::new(*user.id(), org_id.clone(), OrgRole::Owner);
        memberships.save(&mut membership).await.unwrap();

        user.login(&password, &hasher).unwrap();
        users.save(&mut user).await.unwrap();

        assert!(
            memberships
                .find(user.id(), &org_id)
                .await
                .unwrap()
                .is_some()
        );
    }
}
