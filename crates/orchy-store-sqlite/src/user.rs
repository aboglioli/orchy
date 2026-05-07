use std::str::FromStr;

use async_trait::async_trait;
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::user::{Email, HashedPassword, RestoreUser, User, UserId, UserStore};

use crate::{SqliteConn, events};

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
        let mut conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let tx = conn.transaction().map_err(crate::error::store_err)?;

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
                user.id().to_string(),
                user.email().as_str(),
                user.password_hash().as_str(),
                user.is_active() as i32,
                user.is_platform_admin() as i32,
                user.created_at().to_rfc3339(),
                user.updated_at().to_rfc3339(),
            ],
        )
        .map_err(crate::error::store_err)?;

        let events = user.drain_events();
        events::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;

        let row = conn
            .query_row(
                "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at
                 FROM users WHERE id = ?1",
                rusqlite::params![id.to_string()],
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
            .map_err(crate::error::store_err)?;

        match row {
            Some((
                id,
                email,
                password_hash,
                is_active,
                is_platform_admin,
                created_at,
                updated_at,
            )) => {
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

                Ok(Some(User::restore(RestoreUser {
                    id,
                    email,
                    password_hash,
                    is_active: is_active != 0,
                    is_platform_admin: is_platform_admin != 0,
                    created_at,
                    updated_at,
                })))
            }
            None => Ok(None),
        }
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;

        let row = conn
            .query_row(
                "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at
                 FROM users WHERE email = ?1",
                rusqlite::params![email.as_str()],
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
            .map_err(crate::error::store_err)?;

        match row {
            Some((
                id,
                email,
                password_hash,
                is_active,
                is_platform_admin,
                created_at,
                updated_at,
            )) => {
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

                Ok(Some(User::restore(RestoreUser {
                    id,
                    email,
                    password_hash,
                    is_active: is_active != 0,
                    is_platform_admin: is_platform_admin != 0,
                    created_at,
                    updated_at,
                })))
            }
            None => Ok(None),
        }
    }

    async fn list_all(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;

        let mut stmt = conn
            .prepare(
                "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at
                 FROM users ORDER BY created_at DESC"
            )
            .map_err(crate::error::store_err)?;

        let rows = stmt
            .query_map([], |row| {
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
            .map_err(crate::error::store_err)?;

        let mut users = Vec::new();
        for row in rows {
            let (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at) =
                row.map_err(crate::error::store_err)?;

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

            users.push(User::restore(RestoreUser {
                id,
                email,
                password_hash,
                is_active: is_active != 0,
                is_platform_admin: is_platform_admin != 0,
                created_at,
                updated_at,
            }));
        }

        Ok(users)
    }
}

#[cfg(test)]
mod tests {
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
        let users = SqliteUserStore::new(conn.clone());
        let orgs = SqliteOrganizationStore::new(conn.clone());
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
