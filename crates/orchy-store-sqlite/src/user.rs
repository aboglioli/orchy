use std::str::FromStr;

use async_trait::async_trait;
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result};
use orchy_core::user::{Email, HashedPassword, RestoreUser, User, UserId, UserStore};

use crate::SqliteBackend;

#[async_trait]
impl UserStore for SqliteBackend {
    async fn save(&self, user: &mut User) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO users (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = user.drain_events();
        crate::events::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

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
            .map_err(|e| Error::Store(e.to_string()))?;

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
                let id = UserId::from_str(&id)
                    .map_err(|e| Error::Store(format!("invalid user id in db: {e}")))?;
                let email = Email::new(&email)
                    .map_err(|e| Error::Store(format!("invalid email in db: {e}")))?;
                let password_hash = HashedPassword::new(&password_hash)
                    .map_err(|e| Error::Store(format!("invalid password hash in db: {e}")))?;
                let created_at = created_at
                    .parse()
                    .map_err(|e| Error::Store(format!("invalid created_at in db: {e}")))?;
                let updated_at = updated_at
                    .parse()
                    .map_err(|e| Error::Store(format!("invalid updated_at in db: {e}")))?;

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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

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
            .map_err(|e| Error::Store(e.to_string()))?;

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
                let id = UserId::from_str(&id)
                    .map_err(|e| Error::Store(format!("invalid user id in db: {e}")))?;
                let email = Email::new(&email)
                    .map_err(|e| Error::Store(format!("invalid email in db: {e}")))?;
                let password_hash = HashedPassword::new(&password_hash)
                    .map_err(|e| Error::Store(format!("invalid password hash in db: {e}")))?;
                let created_at = created_at
                    .parse()
                    .map_err(|e| Error::Store(format!("invalid created_at in db: {e}")))?;
                let updated_at = updated_at
                    .parse()
                    .map_err(|e| Error::Store(format!("invalid updated_at in db: {e}")))?;

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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at 
                 FROM users ORDER BY created_at DESC"
            )
            .map_err(|e| Error::Store(e.to_string()))?;

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
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut users = Vec::new();
        for row in rows {
            let (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at) =
                row.map_err(|e| Error::Store(e.to_string()))?;

            let id = UserId::from_str(&id)
                .map_err(|e| Error::Store(format!("invalid user id in db: {e}")))?;
            let email = Email::new(&email)
                .map_err(|e| Error::Store(format!("invalid email in db: {e}")))?;
            let password_hash = HashedPassword::new(&password_hash)
                .map_err(|e| Error::Store(format!("invalid password hash in db: {e}")))?;
            let created_at = created_at
                .parse()
                .map_err(|e| Error::Store(format!("invalid created_at in db: {e}")))?;
            let updated_at = updated_at
                .parse()
                .map_err(|e| Error::Store(format!("invalid updated_at in db: {e}")))?;

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
