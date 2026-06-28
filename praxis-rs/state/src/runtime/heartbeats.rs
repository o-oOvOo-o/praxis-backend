use super::*;
use crate::model::ThreadHeartbeatRow;
use crate::model::datetime_to_millis;
use chrono::Duration as ChronoDuration;

impl StateRuntime {
    pub async fn get_thread_heartbeat(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Option<crate::ThreadHeartbeat>> {
        let row = sqlx::query(
            r#"
SELECT
    thread_id,
    enabled,
    interval_ms,
    next_wake_at_ms,
    last_wake_at_ms,
    controller,
    created_at_ms,
    updated_at_ms
FROM thread_heartbeats
WHERE thread_id = ?
            "#,
        )
        .bind(thread_id.to_string())
        .fetch_optional(self.pool.as_ref())
        .await?;

        row.map(|row| thread_heartbeat_from_row(&row)).transpose()
    }

    pub async fn set_thread_heartbeat(
        &self,
        thread_id: ThreadId,
        enabled: bool,
        interval_ms: Option<i64>,
        controller: Option<&str>,
    ) -> anyhow::Result<Option<crate::ThreadHeartbeat>> {
        if !enabled {
            self.delete_thread_heartbeat(thread_id).await?;
            return Ok(None);
        }

        let interval_ms = interval_ms.unwrap_or(crate::DEFAULT_THREAD_HEARTBEAT_INTERVAL_MS);
        if interval_ms <= 0 {
            anyhow::bail!("thread heartbeat interval must be positive");
        }

        let now = Utc::now();
        let next_wake_at = now
            .checked_add_signed(ChronoDuration::milliseconds(interval_ms))
            .ok_or_else(|| anyhow::anyhow!("thread heartbeat interval is too large"))?;
        let now_ms = datetime_to_millis(now);
        let next_wake_at_ms = datetime_to_millis(next_wake_at);
        let row = sqlx::query(
            r#"
INSERT INTO thread_heartbeats (
    thread_id,
    enabled,
    interval_ms,
    next_wake_at_ms,
    last_wake_at_ms,
    controller,
    created_at_ms,
    updated_at_ms
) VALUES (?, 1, ?, ?, NULL, ?, ?, ?)
ON CONFLICT(thread_id) DO UPDATE SET
    enabled = excluded.enabled,
    interval_ms = excluded.interval_ms,
    next_wake_at_ms = excluded.next_wake_at_ms,
    controller = excluded.controller,
    updated_at_ms = excluded.updated_at_ms
RETURNING
    thread_id,
    enabled,
    interval_ms,
    next_wake_at_ms,
    last_wake_at_ms,
    controller,
    created_at_ms,
    updated_at_ms
            "#,
        )
        .bind(thread_id.to_string())
        .bind(interval_ms)
        .bind(next_wake_at_ms)
        .bind(controller)
        .bind(now_ms)
        .bind(now_ms)
        .fetch_one(self.pool.as_ref())
        .await?;

        thread_heartbeat_from_row(&row).map(Some)
    }

    pub async fn delete_thread_heartbeat(&self, thread_id: ThreadId) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"
DELETE FROM thread_heartbeats
WHERE thread_id = ?
            "#,
        )
        .bind(thread_id.to_string())
        .execute(self.pool.as_ref())
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

fn thread_heartbeat_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> anyhow::Result<crate::ThreadHeartbeat> {
    ThreadHeartbeatRow::try_from_row(row).and_then(crate::ThreadHeartbeat::try_from)
}
