use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::postgres::{AdminDirectoryError as E, CentralStore};

impl CentralStore {
    pub async fn admin_list_users(&self) -> Result<Vec<Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, identity_ref, role, status, created_at,
                    (password_hash IS NOT NULL) AS credential_configured
             FROM users WHERE organization_id = $1 ORDER BY identity_ref, id",
        )
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "identity_ref": row.try_get::<String, _>("identity_ref")?,
                    "role": row.try_get::<String, _>("role")?,
                    "status": row.try_get::<String, _>("status")?,
                    "created_at": row.try_get::<DateTime<Utc>, _>("created_at")?,
                    "credential_configured": row.try_get::<bool, _>("credential_configured")?
                }))
            })
            .collect()
    }

    pub async fn admin_list_audit_events(&self) -> Result<Vec<Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT e.id, e.actor, u.identity_ref AS actor_identity, e.device,
                    e.event_type, e.subject_id, e.occurred_at, e.redacted_metadata
             FROM audit_events e
             LEFT JOIN users u ON u.id = e.actor AND u.organization_id = $1
             WHERE EXISTS (SELECT 1 FROM users au WHERE au.id = e.actor AND au.organization_id = $1)
                OR EXISTS (SELECT 1 FROM devices d WHERE d.id = e.device AND d.organization_id = $1)
             ORDER BY e.occurred_at DESC, e.id DESC LIMIT 100",
        )
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "actor_id": row.try_get::<Option<Uuid>, _>("actor")?,
                    "actor_identity": row.try_get::<Option<String>, _>("actor_identity")?,
                    "device_id": row.try_get::<Option<Uuid>, _>("device")?,
                    "event_type": row.try_get::<String, _>("event_type")?,
                    "subject_id": row.try_get::<Option<Uuid>, _>("subject_id")?,
                    "occurred_at": row.try_get::<DateTime<Utc>, _>("occurred_at")?,
                    "redacted_metadata": row.try_get::<Value, _>("redacted_metadata")?
                }))
            })
            .collect()
    }

    pub async fn admin_create_user(
        &self,
        actor_id: Uuid,
        identity_ref: &str,
        role: &str,
        password: &str,
    ) -> Result<Value, E> {
        let identity_ref = identity_ref.trim();
        if identity_ref.is_empty()
            || identity_ref.chars().count() > 254
            || identity_ref.chars().any(char::is_control)
            || !valid_role(role)
            || password.chars().count() < super::postgres::MIN_PASSWORD_LENGTH
        {
            return Err(E::InvalidFields);
        }
        let password_hash = Self::hash_user_password(password).map_err(|_| E::InvalidFields)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM organizations WHERE id = $1 FOR UPDATE")
            .bind(self.organization_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(E::ActorNotAuthorized)?;
        require_admin(&mut tx, self.organization_id, actor_id).await?;
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM users WHERE organization_id = $1
             AND lower(btrim(identity_ref)) = lower(btrim($2)))",
        )
        .bind(self.organization_id)
        .bind(identity_ref)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            return Err(E::IdentityConflict);
        }
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
             VALUES ($1, $2, $3, 'ACTIVE', $4, $5)",
        )
        .bind(user_id)
        .bind(self.organization_id)
        .bind(role)
        .bind(identity_ref)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
        audit(
            &mut tx,
            actor_id,
            user_id,
            "USER_CREATED",
            json!({"role": role, "status": "ACTIVE"}),
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"id": user_id, "identity_ref": identity_ref, "role": role,
                  "status": "ACTIVE", "credential_configured": true}),
        )
    }

    pub async fn admin_update_user(
        &self,
        actor_id: Uuid,
        user_id: Uuid,
        role: Option<&str>,
        status: Option<&str>,
    ) -> Result<Value, E> {
        if role.is_some_and(|value| !valid_role(value))
            || status.is_some_and(|value| !matches!(value, "ACTIVE" | "REVOKED"))
            || (role.is_none() && status.is_none())
        {
            return Err(E::InvalidFields);
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM organizations WHERE id = $1 FOR UPDATE")
            .bind(self.organization_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(E::ActorNotAuthorized)?;
        require_admin(&mut tx, self.organization_id, actor_id).await?;
        let row = sqlx::query(
            "SELECT role, status FROM users WHERE id = $1 AND organization_id = $2 FOR UPDATE",
        )
        .bind(user_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(E::UserNotFound)?;
        let old_role: String = row.try_get("role")?;
        let old_status: String = row.try_get("status")?;
        let next_role = role.unwrap_or(&old_role);
        let next_status = status.unwrap_or(&old_status);
        if actor_id == user_id && (next_role != old_role || next_status != old_status) {
            return Err(E::SelfModification);
        }
        if old_role == "ADMIN"
            && old_status == "ACTIVE"
            && (next_role != "ADMIN" || next_status != "ACTIVE")
        {
            let admins = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM users WHERE organization_id = $1
                 AND role = 'ADMIN' AND status = 'ACTIVE'",
            )
            .bind(self.organization_id)
            .fetch_one(&mut *tx)
            .await?;
            if admins <= 1 {
                return Err(E::LastAdministrator);
            }
        }
        sqlx::query(
            "UPDATE users SET role = $1, status = $2 WHERE id = $3 AND organization_id = $4",
        )
        .bind(next_role)
        .bind(next_status)
        .bind(user_id)
        .bind(self.organization_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = now() WHERE organization_id = $1
             AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(self.organization_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE offline_grants SET revoked_at = now()
             WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        audit(&mut tx, actor_id, user_id, "USER_ACCESS_UPDATED", json!({
            "role": next_role, "status": next_status, "sessions_revoked": true, "offline_grants_revoked": true
        })).await?;
        tx.commit().await?;
        Ok(json!({"id": user_id, "role": next_role, "status": next_status}))
    }

    pub async fn admin_list_devices(&self) -> Result<Vec<Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, public_key_id, state, last_seen, sync_cursor FROM devices
             WHERE organization_id = $1 ORDER BY id",
        )
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "public_key_id": row.try_get::<String, _>("public_key_id")?,
                    "state": row.try_get::<String, _>("state")?,
                    "last_seen": row.try_get::<Option<DateTime<Utc>>, _>("last_seen")?,
                    "sync_cursor": row.try_get::<i64, _>("sync_cursor")?
                }))
            })
            .collect()
    }

    pub async fn admin_list_offline_grants(&self) -> Result<Vec<Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT g.id, g.user_id, g.device_id, g.scopes, g.limits, g.issued_at,
                    g.expires_at, g.revoked_at
             FROM offline_grants g JOIN users u ON u.id = g.user_id
             WHERE u.organization_id = $1 ORDER BY g.issued_at DESC, g.id",
        )
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "user_id": row.try_get::<Uuid, _>("user_id")?,
                    "device_id": row.try_get::<Uuid, _>("device_id")?,
                    "scopes": row.try_get::<Value, _>("scopes")?,
                    "limits": row.try_get::<Value, _>("limits")?,
                    "issued_at": row.try_get::<DateTime<Utc>, _>("issued_at")?,
                    "expires_at": row.try_get::<DateTime<Utc>, _>("expires_at")?,
                    "revoked_at": row.try_get::<Option<DateTime<Utc>>, _>("revoked_at")?
                }))
            })
            .collect()
    }

    pub async fn admin_revoke_offline_grant(
        &self,
        actor_id: Uuid,
        grant_id: Uuid,
    ) -> Result<(), E> {
        let mut tx = self.pool.begin().await?;
        require_admin(&mut tx, self.organization_id, actor_id).await?;
        let updated = sqlx::query(
            "UPDATE offline_grants g SET revoked_at = now()
             FROM users u WHERE g.id = $1 AND g.user_id = u.id
               AND u.organization_id = $2 AND g.revoked_at IS NULL",
        )
        .bind(grant_id)
        .bind(self.organization_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM offline_grants g JOIN users u ON u.id = g.user_id
                 WHERE g.id = $1 AND u.organization_id = $2)",
            )
            .bind(grant_id)
            .bind(self.organization_id)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                return Err(E::ResourceNotFound);
            }
        } else {
            audit(
                &mut tx,
                actor_id,
                grant_id,
                "OFFLINE_GRANT_REVOKED",
                json!({}),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn require_admin(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    actor_id: Uuid,
) -> Result<(), E> {
    let role = sqlx::query_scalar::<_, String>(
        "SELECT role FROM users WHERE id = $1 AND organization_id = $2
         AND status = 'ACTIVE' FOR UPDATE",
    )
    .bind(actor_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(E::ActorNotAuthorized)?;
    if role != "ADMIN" {
        return Err(E::ActorNotAuthorized);
    }
    Ok(())
}

async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor_id: Uuid,
    subject_id: Uuid,
    event: &str,
    metadata: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_events (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
         VALUES ($1, $2, $3, $4, now(), $5)",
    )
    .bind(Uuid::new_v4())
    .bind(actor_id)
    .bind(subject_id)
    .bind(event)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn valid_role(role: &str) -> bool {
    matches!(
        role,
        "ADMIN" | "BILLING_AGENT" | "CASHIER" | "SUPERVISOR" | "AUDITOR_READONLY"
    )
}
