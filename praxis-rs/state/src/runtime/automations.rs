use super::*;
use crate::model::AutomationRow;
use crate::model::AutomationRunRow;
use crate::model::datetime_to_millis;
use uuid::Uuid;

impl StateRuntime {
    pub async fn create_automation(
        &self,
        params: &AutomationCreateParams,
    ) -> anyhow::Result<Automation> {
        let automation_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_ms = datetime_to_millis(now);
        let next_run_at_ms = params.next_run_at.map(datetime_to_millis);
        let schedule_json = serde_json::to_string(&params.schedule_json)?;
        let config_json = serde_json::to_string(&params.config_json)?;
        sqlx::query(
            r#"
INSERT INTO automations (
    automation_id,
    name,
    enabled,
    kind,
    prompt,
    thread_id,
    schedule_json,
    config_json,
    next_run_at_ms,
    last_run_at_ms,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
            "#,
        )
        .bind(automation_id.as_str())
        .bind(params.name.as_str())
        .bind(i64::from(params.enabled))
        .bind(params.kind.as_str())
        .bind(params.prompt.as_str())
        .bind(params.thread_id.as_deref())
        .bind(schedule_json)
        .bind(config_json)
        .bind(next_run_at_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(self.pool.as_ref())
        .await?;

        self.get_automation(automation_id.as_str())
            .await?
            .ok_or_else(|| anyhow::anyhow!("failed to load created automation {automation_id}"))
    }

    pub async fn get_automation(&self, automation_id: &str) -> anyhow::Result<Option<Automation>> {
        let row = sqlx::query(automation_select_sql("WHERE automation_id = ?").as_str())
            .bind(automation_id)
            .fetch_optional(self.pool.as_ref())
            .await?;
        row.map(|row| AutomationRow::try_from_row(&row).and_then(Automation::try_from))
            .transpose()
    }

    pub async fn list_automations(
        &self,
        include_disabled: bool,
    ) -> anyhow::Result<Vec<Automation>> {
        let where_clause = if include_disabled {
            ""
        } else {
            "WHERE enabled = 1"
        };
        let sql = automation_select_sql(where_clause);
        let rows = sqlx::query(sql.as_str())
            .fetch_all(self.pool.as_ref())
            .await?;
        rows.into_iter()
            .map(|row| AutomationRow::try_from_row(&row).and_then(Automation::try_from))
            .collect()
    }

    pub async fn update_automation(
        &self,
        automation_id: &str,
        update: AutomationUpdate,
    ) -> anyhow::Result<Option<Automation>> {
        let existing = self.get_automation(automation_id).await?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        let now_ms = datetime_to_millis(Utc::now());
        let name = update.name.unwrap_or(existing.name);
        let enabled = update.enabled.unwrap_or(existing.enabled);
        let kind = update.kind.unwrap_or(existing.kind);
        let prompt = update.prompt.unwrap_or(existing.prompt);
        let thread_id = update.thread_id.unwrap_or(existing.thread_id);
        let schedule_json = update.schedule_json.unwrap_or(existing.schedule_json);
        let config_json = update.config_json.unwrap_or(existing.config_json);
        let next_run_at = update.next_run_at.unwrap_or(existing.next_run_at);
        let schedule_json = serde_json::to_string(&schedule_json)?;
        let config_json = serde_json::to_string(&config_json)?;
        sqlx::query(
            r#"
UPDATE automations
SET
    name = ?,
    enabled = ?,
    kind = ?,
    prompt = ?,
    thread_id = ?,
    schedule_json = ?,
    config_json = ?,
    next_run_at_ms = ?,
    updated_at_ms = ?
WHERE automation_id = ?
            "#,
        )
        .bind(name.as_str())
        .bind(i64::from(enabled))
        .bind(kind.as_str())
        .bind(prompt.as_str())
        .bind(thread_id.as_deref())
        .bind(schedule_json)
        .bind(config_json)
        .bind(next_run_at.map(datetime_to_millis))
        .bind(now_ms)
        .bind(automation_id)
        .execute(self.pool.as_ref())
        .await?;

        self.get_automation(automation_id).await
    }

    pub async fn delete_automation(&self, automation_id: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
DELETE FROM automation_runs
WHERE automation_id = ?
            "#,
        )
        .bind(automation_id)
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query(
            r#"
DELETE FROM automations
WHERE automation_id = ?
            "#,
        )
        .bind(automation_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn create_automation_run(
        &self,
        params: &AutomationRunCreateParams,
    ) -> anyhow::Result<Option<AutomationRun>> {
        if self
            .get_automation(params.automation_id.as_str())
            .await?
            .is_none()
        {
            return Ok(None);
        }
        let run_id = Uuid::new_v4().to_string();
        let now_ms = datetime_to_millis(Utc::now());
        let metadata_json = serde_json::to_string(&params.metadata_json)?;
        sqlx::query(
            r#"
INSERT INTO automation_runs (
    run_id,
    automation_id,
    status,
    trigger_kind,
    thread_id,
    turn_id,
    started_at_ms,
    completed_at_ms,
    error,
    metadata_json
) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?)
            "#,
        )
        .bind(run_id.as_str())
        .bind(params.automation_id.as_str())
        .bind(params.status.as_str())
        .bind(params.trigger.as_str())
        .bind(params.thread_id.as_deref())
        .bind(params.turn_id.as_deref())
        .bind(now_ms)
        .bind(metadata_json)
        .execute(self.pool.as_ref())
        .await?;

        if params.thread_id.is_some()
            || params.turn_id.is_some()
            || params.status == AutomationRunStatus::Running
        {
            sqlx::query(
                r#"
UPDATE automations
SET last_run_at_ms = ?, updated_at_ms = ?
WHERE automation_id = ?
                "#,
            )
            .bind(now_ms)
            .bind(now_ms)
            .bind(params.automation_id.as_str())
            .execute(self.pool.as_ref())
            .await?;
        }

        self.get_automation_run(run_id.as_str()).await
    }

    pub async fn mark_automation_run_running(
        &self,
        run_id: &str,
        thread_id: &str,
        turn_id: &str,
    ) -> anyhow::Result<Option<AutomationRun>> {
        let now_ms = datetime_to_millis(Utc::now());
        sqlx::query(
            r#"
UPDATE automation_runs
SET
    status = ?,
    thread_id = ?,
    turn_id = ?,
    completed_at_ms = NULL,
    error = NULL
WHERE run_id = ?
            "#,
        )
        .bind(AutomationRunStatus::Running.as_str())
        .bind(thread_id)
        .bind(turn_id)
        .bind(run_id)
        .execute(self.pool.as_ref())
        .await?;

        sqlx::query(
            r#"
UPDATE automations
SET last_run_at_ms = ?, updated_at_ms = ?
WHERE automation_id = (
    SELECT automation_id
    FROM automation_runs
    WHERE run_id = ?
)
            "#,
        )
        .bind(now_ms)
        .bind(now_ms)
        .bind(run_id)
        .execute(self.pool.as_ref())
        .await?;

        self.get_automation_run(run_id).await
    }

    pub async fn finish_automation_run(
        &self,
        run_id: &str,
        status: AutomationRunStatus,
        error: Option<&str>,
    ) -> anyhow::Result<Option<AutomationRun>> {
        if matches!(
            status,
            AutomationRunStatus::Queued | AutomationRunStatus::Running
        ) {
            anyhow::bail!("automation run finish status must be terminal");
        }
        let now_ms = datetime_to_millis(Utc::now());
        sqlx::query(
            r#"
UPDATE automation_runs
SET
    status = ?,
    completed_at_ms = ?,
    error = ?
WHERE run_id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(now_ms)
        .bind(error)
        .bind(run_id)
        .execute(self.pool.as_ref())
        .await?;

        self.get_automation_run(run_id).await
    }

    pub async fn finish_automation_runs_for_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
        status: AutomationRunStatus,
        error: Option<&str>,
    ) -> anyhow::Result<Vec<AutomationRun>> {
        if matches!(
            status,
            AutomationRunStatus::Queued | AutomationRunStatus::Running
        ) {
            anyhow::bail!("automation run finish status must be terminal");
        }
        let rows = sqlx::query(
            automation_run_select_sql(
                "WHERE thread_id = ? AND turn_id = ? AND status IN ('queued', 'running')",
            )
            .as_str(),
        )
        .bind(thread_id)
        .bind(turn_id)
        .fetch_all(self.pool.as_ref())
        .await?;
        let run_ids: Vec<String> = rows
            .iter()
            .map(|row| row.try_get::<String, _>("run_id"))
            .collect::<Result<Vec<_>, _>>()?;
        sqlx::query(
            r#"
UPDATE automation_runs
SET
    status = ?,
    completed_at_ms = ?,
    error = ?
WHERE thread_id = ?
  AND turn_id = ?
  AND status IN ('queued', 'running')
            "#,
        )
        .bind(status.as_str())
        .bind(datetime_to_millis(Utc::now()))
        .bind(error)
        .bind(thread_id)
        .bind(turn_id)
        .execute(self.pool.as_ref())
        .await?;
        let mut runs = Vec::with_capacity(run_ids.len());
        for run_id in run_ids {
            if let Some(run) = self.get_automation_run(run_id.as_str()).await? {
                runs.push(run);
            }
        }
        Ok(runs)
    }

    pub async fn get_automation_run(&self, run_id: &str) -> anyhow::Result<Option<AutomationRun>> {
        let row = sqlx::query(automation_run_select_sql("WHERE run_id = ?").as_str())
            .bind(run_id)
            .fetch_optional(self.pool.as_ref())
            .await?;
        row.map(|row| AutomationRunRow::try_from_row(&row).and_then(AutomationRun::try_from))
            .transpose()
    }

    pub async fn list_automation_runs(
        &self,
        automation_id: &str,
        limit: Option<u32>,
    ) -> anyhow::Result<Vec<AutomationRun>> {
        let limit = i64::from(limit.unwrap_or(50).clamp(1, 200));
        let sql = format!(
            "{} WHERE automation_id = ? ORDER BY started_at_ms DESC LIMIT ?",
            automation_run_select_sql("")
        );
        let rows = sqlx::query(sql.as_str())
            .bind(automation_id)
            .bind(limit)
            .fetch_all(self.pool.as_ref())
            .await?;
        rows.into_iter()
            .map(|row| AutomationRunRow::try_from_row(&row).and_then(AutomationRun::try_from))
            .collect()
    }

    pub async fn list_due_automations(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> anyhow::Result<Vec<Automation>> {
        let rows = sqlx::query(
            r#"
SELECT
    automation_id,
    name,
    enabled,
    kind,
    thread_id,
    prompt,
    schedule_json,
    config_json,
    next_run_at_ms,
    last_run_at_ms,
    created_at_ms,
    updated_at_ms
FROM automations
WHERE enabled = 1
  AND next_run_at_ms IS NOT NULL
  AND next_run_at_ms <= ?
ORDER BY next_run_at_ms ASC, updated_at_ms ASC
LIMIT ?
            "#,
        )
        .bind(datetime_to_millis(now))
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(self.pool.as_ref())
        .await?;
        rows.into_iter()
            .map(|row| AutomationRow::try_from_row(&row).and_then(Automation::try_from))
            .collect()
    }

    pub async fn update_automation_schedule_mark(
        &self,
        automation_id: &str,
        next_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Option<Automation>> {
        let now_ms = datetime_to_millis(Utc::now());
        sqlx::query(
            r#"
UPDATE automations
SET
    last_run_at_ms = ?,
    next_run_at_ms = ?,
    updated_at_ms = ?
WHERE automation_id = ?
            "#,
        )
        .bind(now_ms)
        .bind(next_run_at.map(datetime_to_millis))
        .bind(now_ms)
        .bind(automation_id)
        .execute(self.pool.as_ref())
        .await?;
        self.get_automation(automation_id).await
    }

    pub async fn fail_stale_automation_runs(
        &self,
        message: &str,
    ) -> anyhow::Result<Vec<AutomationRun>> {
        let rows = sqlx::query(
            automation_run_select_sql("WHERE status IN ('queued', 'running')").as_str(),
        )
        .fetch_all(self.pool.as_ref())
        .await?;
        let run_ids: Vec<String> = rows
            .iter()
            .map(|row| row.try_get::<String, _>("run_id"))
            .collect::<Result<Vec<_>, _>>()?;
        sqlx::query(
            r#"
UPDATE automation_runs
SET
    status = ?,
    completed_at_ms = ?,
    error = ?
WHERE status IN ('queued', 'running')
            "#,
        )
        .bind(AutomationRunStatus::Failed.as_str())
        .bind(datetime_to_millis(Utc::now()))
        .bind(message)
        .execute(self.pool.as_ref())
        .await?;
        let mut runs = Vec::with_capacity(run_ids.len());
        for run_id in run_ids {
            if let Some(run) = self.get_automation_run(run_id.as_str()).await? {
                runs.push(run);
            }
        }
        Ok(runs)
    }
}

fn automation_select_sql(where_clause: &str) -> String {
    format!(
        r#"
SELECT
    automation_id,
    name,
    enabled,
    kind,
    thread_id,
    prompt,
    schedule_json,
    config_json,
    next_run_at_ms,
    last_run_at_ms,
    created_at_ms,
    updated_at_ms
FROM automations
{where_clause}
ORDER BY updated_at_ms DESC, created_at_ms DESC
        "#
    )
}

fn automation_run_select_sql(where_clause: &str) -> String {
    format!(
        r#"
SELECT
    run_id,
    automation_id,
    status,
    trigger_kind,
    thread_id,
    turn_id,
    started_at_ms,
    completed_at_ms,
    error,
    metadata_json
FROM automation_runs
{where_clause}
        "#
    )
}
