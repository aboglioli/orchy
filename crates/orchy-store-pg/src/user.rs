use async_trait::async_trait;
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    Email, HashedPassword, MembershipId, OrgMembership, OrgMembershipStore, OrgRole,
    RestoreOrgMembership, RestoreUser, User, UserId, UserStore,
};
use orchy_events::io::Writer;

use crate::events::PgEventWriter;

pub struct PgUserStore {
    pool: sqlx::PgPool,
}

impl PgUserStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserStore for PgUserStore {
    async fn save(&self, user: &mut User) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

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
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = user.drain_events();
        PgEventWriter::new_tx(&mut tx)
            .write_all(&events)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let row: Option<(uuid::Uuid, String, String, bool, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at FROM users WHERE id = $1"
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
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
                let id = UserId::from_uuid(id);
                let email = Email::new(&email)
                    .map_err(|e| Error::Store(format!("invalid email in db: {e}")))?;
                let password_hash = HashedPassword::new(&password_hash)
                    .map_err(|e| Error::Store(format!("invalid password hash in db: {e}")))?;

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
        let row: Option<(uuid::Uuid, String, String, bool, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at FROM users WHERE email = $1"
        )
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
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
                let id = UserId::from_uuid(id);
                let email = Email::new(&email)
                    .map_err(|e| Error::Store(format!("invalid email in db: {e}")))?;
                let password_hash = HashedPassword::new(&password_hash)
                    .map_err(|e| Error::Store(format!("invalid password hash in db: {e}")))?;

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
        let rows: Vec<(uuid::Uuid, String, String, bool, bool, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, email, password_hash, is_active, is_platform_admin, created_at, updated_at FROM users ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut users = Vec::new();
        for (id, email, password_hash, is_active, is_platform_admin, created_at, updated_at) in rows
        {
            let id = UserId::from_uuid(id);
            let email = Email::new(&email)
                .map_err(|e| Error::Store(format!("invalid email in db: {e}")))?;
            let password_hash = HashedPassword::new(&password_hash)
                .map_err(|e| Error::Store(format!("invalid password hash in db: {e}")))?;

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
    pool: sqlx::PgPool,
}

impl PgOrgMembershipStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrgMembershipStore for PgOrgMembershipStore {
    async fn save(&self, membership: &OrgMembership) -> Result<()> {
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
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &MembershipId) -> Result<Option<OrgMembership>> {
        let row: Option<(
            uuid::Uuid,
            uuid::Uuid,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        )> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row_to_membership(row)
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        let rows: Vec<(uuid::Uuid, uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE user_id = $1 ORDER BY created_at DESC"
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut memberships = Vec::new();
        for row in rows {
            if let Some(m) = row_to_membership(Some(row))? {
                memberships.push(m);
            }
        }

        Ok(memberships)
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<OrgMembership>> {
        let rows: Vec<(uuid::Uuid, uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE org_id = $1 ORDER BY created_at DESC"
        )
        .bind(org_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

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
        let row: Option<(uuid::Uuid, uuid::Uuid, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, user_id, org_id, role, created_at FROM org_memberships WHERE user_id = $1 AND org_id = $2"
        )
        .bind(user_id.as_uuid())
        .bind(org_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row_to_membership(row)
    }

    async fn delete(&self, id: &MembershipId) -> Result<()> {
        sqlx::query("DELETE FROM org_memberships WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_membership(
    row: Option<(
        uuid::Uuid,
        uuid::Uuid,
        String,
        String,
        chrono::DateTime<chrono::Utc>,
    )>,
) -> Result<Option<OrgMembership>> {
    match row {
        Some((id, user_id, org_id, role, created_at)) => {
            let id = MembershipId::from_uuid(id);
            let user_id = UserId::from_uuid(user_id);
            let org_id = OrganizationId::new(&org_id)
                .map_err(|e| Error::Store(format!("invalid org id in db: {e}")))?;
            let role = role
                .parse::<OrgRole>()
                .map_err(|e| Error::Store(format!("invalid role in db: {e}")))?;

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
