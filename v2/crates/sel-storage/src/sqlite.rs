use sel_contracts::{SyncEnvelope, SyncEventResult};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::{str::FromStr, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct LocalStore {
    pub pool: SqlitePool,
    pub organization_id: Uuid,
    pub device_id: Uuid,
    write_lock: Arc<Mutex<()>>,
}

impl LocalStore {
    pub async fn connect(
        database_url: &str,
        organization_id: Uuid,
        device_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        crate::migrate_sqlite(&pool).await?;
        ensure_identity(&pool, organization_id, device_id).await?;
        Ok(Self {
            pool,
            organization_id,
            device_id,
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn record_command(&self, event: &SyncEnvelope) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let payload_sha256 = payload_digest(&event.payload);
        if payload_sha256 != event.payload_sha256 {
            return Err(sqlx::Error::Protocol(
                "sync event payload hash does not match".into(),
            ));
        }
        if let Some(existing) =
            sqlx::query("SELECT payload_sha256 FROM command_journal WHERE event_id = $1")
                .bind(event.event_id.to_string())
                .fetch_optional(&mut *tx)
                .await?
        {
            let existing_hash: String = existing.try_get("payload_sha256")?;
            if existing_hash != event.payload_sha256 {
                return Err(sqlx::Error::Protocol(
                    "event id was reused with another payload".into(),
                ));
            }
            tx.commit().await?;
            return Ok(());
        }
        let envelope_json = serde_json::to_string(event)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid event JSON: {error}")))?;
        sqlx::query(
            "INSERT INTO command_journal
             (event_id, device_id, aggregate_id, command_type, payload_json, payload_sha256, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, 'LOCAL_PENDING', datetime('now'))",
        )
        .bind(event.event_id.to_string())
        .bind(event.device_id.to_string())
        .bind(event.aggregate_id.to_string())
        .bind(&event.command_type)
        .bind(event.payload.to_string())
        .bind(&event.payload_sha256)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO outbox (event_id, envelope_json, state, attempts)
             VALUES ($1, $2, 'PENDING', 0)",
        )
        .bind(event.event_id.to_string())
        .bind(envelope_json)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn pending_count(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE state = 'PENDING'")
            .fetch_one(&self.pool)
            .await
    }

    pub async fn journal_count(&self, event_id: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM command_journal WHERE event_id = $1")
            .bind(event_id.to_string())
            .fetch_one(&self.pool)
            .await
    }

    pub async fn acknowledge(
        &self,
        event_id: Uuid,
        result: &SyncEventResult,
    ) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let result_json = serde_json::to_string(result)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid sync result: {error}")))?;
        let updated = sqlx::query(
            "UPDATE outbox SET state = 'ACKED', last_error = NULL WHERE event_id = $1 AND state = 'PENDING'",
        )
        .bind(event_id.to_string())
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        sqlx::query("UPDATE command_journal SET status = 'ACKED' WHERE event_id = $1")
            .bind(event_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO sync_inbox (device_id, event_id, result_json, received_at)
             VALUES ($1, $2, $3, datetime('now'))
             ON CONFLICT(device_id, event_id) DO UPDATE SET result_json = excluded.result_json",
        )
        .bind(self.device_id.to_string())
        .bind(event_id.to_string())
        .bind(result_json)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn conflict(&self, event_id: Uuid, reason: &str) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE outbox SET state = 'CONFLICT', last_error = $2 WHERE event_id = $1")
            .bind(event_id.to_string())
            .bind(reason)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE command_journal SET status = 'CONFLICT' WHERE event_id = $1")
            .bind(event_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn ensure_identity(
    pool: &SqlitePool,
    organization_id: Uuid,
    device_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for (key, expected, label) in [
        (
            "organization_id",
            organization_id.to_string(),
            "organization identity",
        ),
        ("device_id", device_id.to_string(), "device identity"),
    ] {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT value FROM sync_state WHERE key = $1")
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(stored) = stored {
            if stored != expected {
                return Err(sqlx::Error::Protocol(format!(
                    "{label} changed in the existing local database"
                )));
            }
        } else {
            sqlx::query("INSERT INTO sync_state (key, value) VALUES ($1, $2)")
                .bind(key)
                .bind(expected)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await
}

fn payload_digest(payload: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(payload).expect("JSON serialization cannot fail"));
    hex::encode(digest.finalize())
}
