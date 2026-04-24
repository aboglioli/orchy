use std::str::FromStr;

use async_trait::async_trait;
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    MembershipId, OrgMembership, OrgMembershipStore, OrgRole, RestoreOrgMembership, UserId,
};

use crate::SqliteConn;

pub struct SqliteOrgMembershipStore {
    conn: SqliteConn,
}

impl SqliteOrgMembershipStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl OrgMembershipStore for SqliteOrgMembershipStore {
    async fn save(&self, membership: &OrgMembership) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO org_memberships (id, user_id, org_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                membership.id().to_string(),
                membership.user_id().to_string(),
                membership.org_id().to_string(),
                membership.role().to_string(),
                membership.created_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &MembershipId) -> Result<Option<OrgMembership>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let row = conn
            .query_row(
                "SELECT id, user_id, org_id, role, created_at
                 FROM org_memberships WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        row_to_membership(row)
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, org_id, role, created_at
                 FROM org_memberships WHERE user_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![user_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut memberships = Vec::new();
        for row in rows {
            if let Some(m) = row_to_membership(Some(row.map_err(|e| Error::Store(e.to_string()))?))?
            {
                memberships.push(m);
            }
        }

        Ok(memberships)
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<OrgMembership>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, user_id, org_id, role, created_at
                 FROM org_memberships WHERE org_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let rows = stmt
            .query_map(rusqlite::params![org_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut memberships = Vec::new();
        for row in rows {
            if let Some(m) = row_to_membership(Some(row.map_err(|e| Error::Store(e.to_string()))?))?
            {
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
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let row = conn
            .query_row(
                "SELECT id, user_id, org_id, role, created_at
                 FROM org_memberships WHERE user_id = ?1 AND org_id = ?2",
                rusqlite::params![user_id.to_string(), org_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        row_to_membership(row)
    }

    async fn delete(&self, id: &MembershipId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "DELETE FROM org_memberships WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_membership(
    row: Option<(String, String, String, String, String)>,
) -> Result<Option<OrgMembership>> {
    match row {
        Some((id, user_id, org_id, role, created_at)) => {
            let id = MembershipId::from_str(&id)
                .map_err(|e| Error::Store(format!("invalid membership id in db: {e}")))?;
            let user_id = UserId::from_str(&user_id)
                .map_err(|e| Error::Store(format!("invalid user id in db: {e}")))?;
            let org_id = OrganizationId::new(&org_id)
                .map_err(|e| Error::Store(format!("invalid org id in db: {e}")))?;
            let role = role
                .parse::<OrgRole>()
                .map_err(|e| Error::Store(format!("invalid role in db: {e}")))?;
            let created_at = created_at
                .parse()
                .map_err(|e| Error::Store(format!("invalid created_at in db: {e}")))?;

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
