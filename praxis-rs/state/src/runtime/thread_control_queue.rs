use super::*;
use crate::model::ThreadControlQueueRow;
use crate::model::datetime_to_millis;
use uuid::Uuid;

impl StateRuntime {
    pub async fn enqueue_thread_control_item(
        &self,
        params: &ThreadControlQueueCreateParams,
    ) -> anyhow::Result<ThreadControlQueueItem> {
        let queue_id = Uuid::new_v4().to_string();
        let now_ms = datetime_to_millis(Utc::now());
        let controller_json = serde_json::to_string(&params.controller_json)?;
        sqlx::query(
            r#"
INSERT INTO thread_control_queue (
    queue_id,
    target_thread_id,
    controller_json,
    text,
    status,
    created_at_ms,
    updated_at_ms,
    dispatched_turn_id,
    error
) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL)
            "#,
        )
        .bind(queue_id.as_str())
        .bind(params.target_thread_id.as_str())
        .bind(controller_json)
        .bind(params.text.as_str())
        .bind(ThreadControlQueueStatus::Queued.as_str())
        .bind(now_ms)
        .bind(now_ms)
        .execute(self.pool.as_ref())
        .await?;

        self.get_thread_control_queue_item(queue_id.as_str())
            .await?
            .ok_or_else(|| anyhow::anyhow!("failed to load created queue item {queue_id}"))
    }

    pub async fn get_thread_control_queue_item(
        &self,
        queue_id: &str,
    ) -> anyhow::Result<Option<ThreadControlQueueItem>> {
        let row = sqlx::query(thread_control_queue_select_sql("WHERE queue_id = ?").as_str())
            .bind(queue_id)
            .fetch_optional(self.pool.as_ref())
            .await?;
        row.map(|row| {
            ThreadControlQueueRow::try_from_row(&row).and_then(ThreadControlQueueItem::try_from)
        })
        .transpose()
    }

    pub async fn list_thread_control_queue(
        &self,
        thread_id: &str,
        include_terminal: bool,
    ) -> anyhow::Result<Vec<ThreadControlQueueItem>> {
        let status_clause = if include_terminal {
            ""
        } else {
            "AND status IN ('queued', 'dispatched')"
        };
        let sql = format!(
            "{} WHERE target_thread_id = ? {status_clause} ORDER BY created_at_ms ASC LIMIT 200",
            thread_control_queue_select_sql("")
        );
        let rows = sqlx::query(sql.as_str())
            .bind(thread_id)
            .fetch_all(self.pool.as_ref())
            .await?;
        rows.into_iter()
            .map(|row| {
                ThreadControlQueueRow::try_from_row(&row).and_then(ThreadControlQueueItem::try_from)
            })
            .collect()
    }

    pub async fn mark_thread_control_queue_dispatched(
        &self,
        queue_id: &str,
        turn_id: &str,
    ) -> anyhow::Result<Option<ThreadControlQueueItem>> {
        self.update_thread_control_queue_status(
            queue_id,
            ThreadControlQueueStatus::Dispatched,
            Some(turn_id),
            None,
        )
        .await
    }

    pub async fn mark_thread_control_queue_failed(
        &self,
        queue_id: &str,
        error: &str,
    ) -> anyhow::Result<Option<ThreadControlQueueItem>> {
        self.update_thread_control_queue_status(
            queue_id,
            ThreadControlQueueStatus::Failed,
            None,
            Some(error),
        )
        .await
    }

    pub async fn cancel_thread_control_queue_item(
        &self,
        thread_id: &str,
        queue_id: &str,
    ) -> anyhow::Result<Option<ThreadControlQueueItem>> {
        let now_ms = datetime_to_millis(Utc::now());
        let result = sqlx::query(
            r#"
UPDATE thread_control_queue
SET
    status = ?,
    updated_at_ms = ?,
    error = NULL
WHERE target_thread_id = ?
  AND queue_id = ?
  AND status = 'queued'
            "#,
        )
        .bind(ThreadControlQueueStatus::Cancelled.as_str())
        .bind(now_ms)
        .bind(thread_id)
        .bind(queue_id)
        .execute(self.pool.as_ref())
        .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_thread_control_queue_item(queue_id).await
    }

    pub async fn cancel_latest_thread_control_queue_item(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<Option<ThreadControlQueueItem>> {
        let mut tx = self.pool.begin().await?;
        let queue_id = sqlx::query_scalar::<_, String>(
            r#"
SELECT queue_id
FROM thread_control_queue
WHERE target_thread_id = ?
  AND status = 'queued'
ORDER BY created_at_ms DESC, updated_at_ms DESC
LIMIT 1
            "#,
        )
        .bind(thread_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(queue_id) = queue_id else {
            tx.commit().await?;
            return Ok(None);
        };

        let now_ms = datetime_to_millis(Utc::now());
        sqlx::query(
            r#"
UPDATE thread_control_queue
SET status = ?, updated_at_ms = ?, error = NULL
WHERE target_thread_id = ?
  AND queue_id = ?
  AND status = 'queued'
            "#,
        )
        .bind(ThreadControlQueueStatus::Cancelled.as_str())
        .bind(now_ms)
        .bind(thread_id)
        .bind(queue_id.as_str())
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(thread_control_queue_select_sql("WHERE queue_id = ?").as_str())
            .bind(queue_id)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        row.map(|row| {
            ThreadControlQueueRow::try_from_row(&row).and_then(ThreadControlQueueItem::try_from)
        })
        .transpose()
    }

    pub async fn flush_thread_control_queue(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<Vec<ThreadControlQueueItem>> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            r#"
SELECT
    queue_id,
    target_thread_id,
    controller_json,
    text,
    status,
    created_at_ms,
    updated_at_ms,
    dispatched_turn_id,
    error
FROM thread_control_queue
WHERE target_thread_id = ?
  AND status = 'queued'
ORDER BY created_at_ms ASC
            "#,
        )
        .bind(thread_id)
        .fetch_all(&mut *tx)
        .await?;
        let queue_ids: Vec<String> = rows
            .iter()
            .map(|row| row.try_get::<String, _>("queue_id"))
            .collect::<Result<Vec<_>, _>>()?;
        let now_ms = datetime_to_millis(Utc::now());
        for queue_id in &queue_ids {
            sqlx::query(
                r#"
UPDATE thread_control_queue
SET
    status = ?,
    updated_at_ms = ?,
    error = NULL
WHERE queue_id = ?
                "#,
            )
            .bind(ThreadControlQueueStatus::Cancelled.as_str())
            .bind(now_ms)
            .bind(queue_id.as_str())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        let mut cancelled = Vec::with_capacity(queue_ids.len());
        for queue_id in queue_ids {
            if let Some(item) = self
                .get_thread_control_queue_item(queue_id.as_str())
                .await?
            {
                cancelled.push(item);
            }
        }
        Ok(cancelled)
    }

    async fn update_thread_control_queue_status(
        &self,
        queue_id: &str,
        status: ThreadControlQueueStatus,
        dispatched_turn_id: Option<&str>,
        error: Option<&str>,
    ) -> anyhow::Result<Option<ThreadControlQueueItem>> {
        sqlx::query(
            r#"
UPDATE thread_control_queue
SET
    status = ?,
    updated_at_ms = ?,
    dispatched_turn_id = COALESCE(?, dispatched_turn_id),
    error = ?
WHERE queue_id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(datetime_to_millis(Utc::now()))
        .bind(dispatched_turn_id)
        .bind(error)
        .bind(queue_id)
        .execute(self.pool.as_ref())
        .await?;
        self.get_thread_control_queue_item(queue_id).await
    }
}

fn thread_control_queue_select_sql(where_clause: &str) -> String {
    format!(
        r#"
SELECT
    queue_id,
    target_thread_id,
    controller_json,
    text,
    status,
    created_at_ms,
    updated_at_ms,
    dispatched_turn_id,
    error
FROM thread_control_queue
{where_clause}
        "#
    )
}
