use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orchy_core::error::{Error, Result, StoreError};
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    Email, HashedPassword, MembershipId, OrgMembership, OrgMembershipStore, OrgRole,
    RestoreOrgMembership, RestoreUser, User, UserId, UserStore,
};
use orchy_events::io::Writer;
use sqlx::PgPool;
use uuid::Uuid;

use crate::events::PgEventWriter;

pub struct PgUserStore {
    pool: PgPool,
}

impl PgUserStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserStore for PgUserStore {
    async fn save(&self, user: &mut User) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(crate::error::store_err)?;

        sqlx::query(
            r#"
            INSERT INTO users (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                email = EXCLUDED.email,
                password_hash = EXCLUDED.password_hash,
                is_active = EXCLUDED.is_active,
                is_platform_admin = EXCLUDED.is_platform_admin,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(user.id().as_uuid())
        .bind(user.email().as_str())
        .bind(user.password_hash().as_str())
        .bind(user.is_active())
        .bind(user.is_platform_admin())
        .bind(user.created_at())
        .bind(user.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(crate::error::store_err)?;

        let events = user.drain_events();
        PgEventWriter::new_tx(&mut tx).write_all(&events).await?;

        tx.commit().await.map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let row: Option<(Uuid, String, String, bool, bool, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at FROM users WHERE id = $1"
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
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
                let id = UserId::from_uuid(id);
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

                Ok(Some(User::restore(RestoreUser {
                    id,
                    email,
                    password_hash,
                    is_active,
                    is_platform_admin,
                    created_at,
                    updated_at,
                })))
            }
            None => Ok(None),
        }
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>> {
        let row: Option<(Uuid, String, String, bool, bool, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at FROM users WHERE email = $1"
        )
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
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
                let id = UserId::from_uuid(id);
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

                Ok(Some(User::restore(RestoreUser {
                    id,
                    email,
                    password_hash,
                    is_active,
                    is_platform_admin,
                    created_at,
                    updated_at,
                })))
            }
            None => Ok(None),
        }
    }

    async fn list_all(&self) -> Result<Vec<User>> {
        let rows: Vec<(Uuid, String, String, bool, bool, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at FROM users ORDER BY created_at DESC LIMIT 1000"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        let mut users = Vec::new();
        for (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at) in rows
        {
            let id = UserId::from_uuid(id);
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

            users.push(User::restore(RestoreUser {
                id,
                email,
                password_hash,
                is_active,
                is_platform_admin,
                created_at,
                updated_at,
            }));
        }

        Ok(users)
    }
}

pub struct PgOrgMembershipStore {
    pool: PgPool,
}

impl PgOrgMembershipStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrgMembershipStore for PgOrgMembershipStore {
    async fn save(&self, membership: &mut OrgMembership) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO org_memberships (id, user_id, org_id, role, created_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE SET
                role = EXCLUDED.role
            "#,
        )
        .bind(membership.id().as_uuid())
        .bind(membership.user_id().as_uuid())
        .bind(membership.org_id().to_string())
        .bind(membership.role().to_string())
        .bind(membership.created_at())
        .execute(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        Ok(())
    }

    async fn find_by_id(&self, id: &MembershipId) -> Result<Option<OrgMembership>> {
        let row: Option<(Uuid, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        row_to_membership(row)
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        let rows: Vec<(Uuid, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1000"
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        let mut memberships = Vec::new();
        for row in rows {
            if let Some(m) = row_to_membership(Some(row))? {
                memberships.push(m);
            }
        }

        Ok(memberships)
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<OrgMembership>> {
        let rows: Vec<(Uuid, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE org_id = $1 ORDER BY created_at DESC LIMIT 1000"
        )
        .bind(org_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        let mut memberships = Vec::new();
        for row in rows {
            if let Some(m) = row_to_membership(Some(row))? {
                memberships.push(m);
            }
        }

        Ok(memberships)
    }

    async fn find(
        &self,
        user_id: &UserId,
        org_id: &OrganizationId,
    ) -> Result<Option<OrgMembership>> {
        let row: Option<(Uuid, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE user_id = $1 AND org_id = $2"
        )
        .bind(user_id.as_uuid())
        .bind(org_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        row_to_membership(row)
    }

    async fn delete(&self, id: &MembershipId) -> Result<()> {
        sqlx::query("DELETE FROM org_memberships WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(crate::error::store_err)?;

        Ok(())
    }
}

fn row_to_membership(
    row: Option<(Uuid, Uuid, String, String, DateTime<Utc>)>,
) -> Result<Option<OrgMembership>> {
    match row {
        Some((id, user_id, org_id, role, created_at)) => {
            let id = MembershipId::from_uuid(id);
            let user_id = UserId::from_uuid(user_id);
            let org_id = OrganizationId::new(&org_id).map_err(|e| {
                Error::Store(StoreError::Decode {
                    table: "users".to_string(),
                    column: "org_id".to_string(),
                    cause: e.to_string(),
                })
            })?;
            let role = role.parse::<OrgRole>().map_err(|e| {
                Error::Store(StoreError::Decode {
                    table: "users".to_string(),
                    column: "role".to_string(),
                    cause: e.to_string(),
                })
            })?;

            Ok(Some(OrgMembership::restore(RestoreOrgMembership {
                id,
                user_id,
                org_id,
                role,
                created_at,
            })))
        }
        None => Ok(None),
    }
}
