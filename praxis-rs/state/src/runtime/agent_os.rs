use super::*;

impl StateRuntime {
    pub async fn upsert_agent_os_thread_snapshot(
        &self,
        thread_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_threads",
            "thread_id",
            thread_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_task_snapshot(
        &self,
        task_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_tasks",
            "task_id",
            task_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_lease_snapshot(
        &self,
        lease_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_leases",
            "lease_id",
            lease_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_ticket_snapshot(
        &self,
        ticket_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_tickets",
            "ticket_id",
            ticket_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_intent_plan_snapshot(
        &self,
        plan_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_intent_plans",
            "plan_id",
            plan_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_command_snapshot(
        &self,
        command_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_commands",
            "command_id",
            command_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_process_snapshot(
        &self,
        process_key: &str,
        process_id: i32,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let runtime_kind = snapshot_json
            .get("runtime_kind")
            .and_then(|value| value.as_str());
        let runtime_owner_id = snapshot_json
            .get("runtime_owner_id")
            .and_then(|value| value.as_str());
        let command_id = snapshot_json
            .get("command_id")
            .and_then(|value| value.as_str());
        let thread_id = snapshot_json
            .get("thread_id")
            .and_then(|value| value.as_str());
        let task_id = snapshot_json
            .get("task_id")
            .and_then(|value| value.as_str());
        let status = snapshot_json.get("status").and_then(|value| value.as_str());
        sqlx::query(
            r#"
INSERT INTO agent_os_processes (
    process_key,
    process_id,
    runtime_kind,
    runtime_owner_id,
    command_id,
    thread_id,
    task_id,
    status,
    snapshot_json,
    updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(process_key) DO UPDATE SET
    process_id = excluded.process_id,
    runtime_kind = excluded.runtime_kind,
    runtime_owner_id = excluded.runtime_owner_id,
    command_id = excluded.command_id,
    thread_id = excluded.thread_id,
    task_id = excluded.task_id,
    status = excluded.status,
    snapshot_json = excluded.snapshot_json,
    updated_at = excluded.updated_at
            "#,
        )
        .bind(process_key)
        .bind(process_id)
        .bind(runtime_kind)
        .bind(runtime_owner_id)
        .bind(command_id)
        .bind(thread_id)
        .bind(task_id)
        .bind(status)
        .bind(serde_json::to_string(snapshot_json)?)
        .bind(Utc::now().timestamp())
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    pub async fn upsert_agent_os_runtime_command_snapshot(
        &self,
        command_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_runtime_commands",
            "command_id",
            command_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_artifact_snapshot(
        &self,
        artifact_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_artifacts",
            "artifact_id",
            artifact_id,
            snapshot_json,
        )
        .await
    }

    pub async fn upsert_agent_os_worker_request_snapshot(
        &self,
        request_id: &str,
        snapshot_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        upsert_snapshot(
            self.pool.as_ref(),
            "agent_os_worker_requests",
            "request_id",
            request_id,
            snapshot_json,
        )
        .await
    }

    pub async fn record_agent_os_event_json(
        &self,
        event_id: &str,
        created_at: i64,
        event_type: &str,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        command_id: Option<&str>,
        payload_json: &serde_json::Value,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
INSERT INTO agent_os_events (
    event_id,
    created_at,
    event_type,
    thread_id,
    task_id,
    command_id,
    payload_json
) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event_id)
        .bind(created_at)
        .bind(event_type)
        .bind(thread_id)
        .bind(task_id)
        .bind(command_id)
        .bind(serde_json::to_string(payload_json)?)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }
}

async fn upsert_snapshot(
    pool: &SqlitePool,
    table: &str,
    id_column: &str,
    id: &str,
    snapshot_json: &serde_json::Value,
) -> anyhow::Result<()> {
    let sql = format!(
        r#"
INSERT INTO {table} ({id_column}, snapshot_json, updated_at)
VALUES (?, ?, ?)
ON CONFLICT({id_column}) DO UPDATE SET
    snapshot_json = excluded.snapshot_json,
    updated_at = excluded.updated_at
        "#
    );
    sqlx::query(sql.as_str())
        .bind(id)
        .bind(serde_json::to_string(snapshot_json)?)
        .bind(Utc::now().timestamp())
        .execute(pool)
        .await?;
    Ok(())
}
