use std::str::FromStr;

use async_trait::async_trait;
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    MembershipId, OrgMembership, OrgMembershipStore, OrgRole, RestoreOrgMembership, UserId,
};

use crate::{SqliteConn, blocking};

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
    async fn save(&self, membership: &mut OrgMembership) -> Result<()> {
        let id = membership.id().to_string();
        let user_id = membership.user_id().to_string();
        let org_id = membership.org_id().to_string();
        let role = membership.role().to_string();
        let created_at = membership.created_at().to_rfc3339();
        blocking(&self.conn, move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO org_memberships (id, user_id, org_id, role, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, user_id, org_id, role, created_at],
            )
            .map_err(crate::error::store_err)?;
            Ok(())
        })
        .await
    }

    async fn find_by_id(&self, id: &MembershipId) -> Result<Option<OrgMembership>> {
        let id = id.to_string();
        let row = blocking(&self.conn, move |conn| {
            conn.query_row(
                "SELECT id, user_id, org_id, role, created_at
                 FROM org_memberships WHERE id = ?1",
                rusqlite::params![id],
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
            .map_err(crate::error::store_err)
        })
        .await?;
        row_to_membership(row)
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        let user_id = user_id.to_string();
        let rows = blocking(&self.conn, move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, org_id, role, created_at
                     FROM org_memberships WHERE user_id = ?1 ORDER BY created_at DESC",
                )
                .map_err(crate::error::store_err)?;

            stmt.query_map(rusqlite::params![user_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(crate::error::store_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(crate::error::store_err)
        })
        .await?;

        let mut memberships = Vec::new();
        for row in rows {
            if let Some(m) = row_to_membership(Some(row))? {
                memberships.push(m);
            }
        }

        Ok(memberships)
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<OrgMembership>> {
        let org_id = org_id.to_string();
        let rows = blocking(&self.conn, move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, org_id, role, created_at
                     FROM org_memberships WHERE org_id = ?1 ORDER BY created_at DESC",
                )
                .map_err(crate::error::store_err)?;

            stmt.query_map(rusqlite::params![org_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(crate::error::store_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(crate::error::store_err)
        })
        .await?;

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
        let user_id = user_id.to_string();
        let org_id = org_id.to_string();
        let row = blocking(&self.conn, move |conn| {
            conn.query_row(
                "SELECT id, user_id, org_id, role, created_at
                 FROM org_memberships WHERE user_id = ?1 AND org_id = ?2",
                rusqlite::params![user_id, org_id],
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
            .map_err(crate::error::store_err)
        })
        .await?;
        row_to_membership(row)
    }

    async fn delete(&self, id: &MembershipId) -> Result<()> {
        let id = id.to_string();
        blocking(&self.conn, move |conn| {
            conn.execute(
                "DELETE FROM org_memberships WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(crate::error::store_err)?;
            Ok(())
        })
        .await
    }
}

fn row_to_membership(
    row: Option<(String, String, String, String, String)>,
) -> Result<Option<OrgMembership>> {
    match row {
        Some((id, user_id, org_id, role, created_at)) => {
            let id = MembershipId::from_str(&id).map_err(|e| {
                Error::Store(StoreError::Other(format!(
                    "invalid membership id in db: {e}"
                )))
            })?;
            let user_id = UserId::from_str(&user_id).map_err(|e| {
                Error::Store(StoreError::Decode {
                    table: "membership".to_string(),
                    column: "user_id".to_string(),
                    cause: e.to_string(),
                })
            })?;
            let org_id = OrganizationId::new(&org_id).map_err(|e| {
                Error::Store(StoreError::Decode {
                    table: "membership".to_string(),
                    column: "org_id".to_string(),
                    cause: e.to_string(),
                })
            })?;
            let role = role.parse::<OrgRole>().map_err(|e| {
                Error::Store(StoreError::Decode {
                    table: "membership".to_string(),
                    column: "role".to_string(),
                    cause: e.to_string(),
                })
            })?;
            let created_at = created_at.parse().map_err(|e: chrono::ParseError| {
                Error::Store(StoreError::Decode {
                    table: "membership".to_string(),
                    column: "created_at".to_string(),
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
