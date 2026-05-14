use super::*;

impl StateRuntime {
    pub async fn create_team(
        &self,
        params: &crate::TeamCreateParams,
    ) -> anyhow::Result<crate::Team> {
        let now = Utc::now().timestamp();
        sqlx::query(
            r#"
INSERT INTO teams (
    id,
    lead_thread_id,
    name,
    objective,
    execution_mode,
    resume_mode,
    created_at,
    updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(params.id.as_str())
        .bind(params.lead_thread_id.as_str())
        .bind(params.name.as_str())
        .bind(params.objective.as_deref())
        .bind(params.execution_mode.as_str())
        .bind(params.resume_mode.as_str())
        .bind(now)
        .bind(now)
        .execute(self.pool.as_ref())
        .await?;

        self.get_team(params.id.as_str())
            .await?
            .ok_or_else(|| anyhow::anyhow!("failed to load created team {}", params.id))
    }

    pub async fn get_team(&self, team_id: &str) -> anyhow::Result<Option<crate::Team>> {
        let row = sqlx::query_as::<_, crate::model::TeamRow>(
            r#"
SELECT
    id,
    lead_thread_id,
    name,
    objective,
    execution_mode,
    resume_mode,
    created_at,
    updated_at
FROM teams
WHERE id = ?
            "#,
        )
        .bind(team_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(crate::Team::try_from).transpose()
    }

    pub async fn get_team_by_lead_thread_id(
        &self,
        lead_thread_id: &str,
    ) -> anyhow::Result<Option<crate::Team>> {
        let row = sqlx::query_as::<_, crate::model::TeamRow>(
            r#"
SELECT
    id,
    lead_thread_id,
    name,
    objective,
    execution_mode,
    resume_mode,
    created_at,
    updated_at
FROM teams
WHERE lead_thread_id = ?
            "#,
        )
        .bind(lead_thread_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(crate::Team::try_from).transpose()
    }

    pub async fn delete_team(&self, team_id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM teams WHERE id = ?")
            .bind(team_id)
            .execute(self.pool.as_ref())
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn list_team_teammates(
        &self,
        team_id: &str,
    ) -> anyhow::Result<Vec<crate::TeamTeammate>> {
        let rows = sqlx::query_as::<_, crate::model::TeamTeammateRow>(
            r#"
SELECT
    team_id,
    teammate_id,
    name,
    role,
    status,
    thread_id,
    last_error,
    created_at,
    updated_at
FROM team_teammates
WHERE team_id = ?
ORDER BY created_at ASC, teammate_id ASC
            "#,
        )
        .bind(team_id)
        .fetch_all(self.pool.as_ref())
        .await?;
        rows.into_iter()
            .map(crate::TeamTeammate::try_from)
            .collect()
    }

    pub async fn get_team_teammate(
        &self,
        team_id: &str,
        teammate_id: &str,
    ) -> anyhow::Result<Option<crate::TeamTeammate>> {
        let row = sqlx::query_as::<_, crate::model::TeamTeammateRow>(
            r#"
SELECT
    team_id,
    teammate_id,
    name,
    role,
    status,
    thread_id,
    last_error,
    created_at,
    updated_at
FROM team_teammates
WHERE team_id = ? AND teammate_id = ?
            "#,
        )
        .bind(team_id)
        .bind(teammate_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(crate::TeamTeammate::try_from).transpose()
    }

    pub async fn get_team_teammate_by_thread_id(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<Option<crate::TeamTeammate>> {
        let row = sqlx::query_as::<_, crate::model::TeamTeammateRow>(
            r#"
SELECT
    team_id,
    teammate_id,
    name,
    role,
    status,
    thread_id,
    last_error,
    created_at,
    updated_at
FROM team_teammates
WHERE thread_id = ?
            "#,
        )
        .bind(thread_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(crate::TeamTeammate::try_from).transpose()
    }

    pub async fn create_team_teammate(
        &self,
        params: &crate::TeamTeammateCreateParams,
    ) -> anyhow::Result<crate::TeamTeammate> {
        let now = Utc::now().timestamp();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
INSERT INTO team_teammates (
    team_id,
    teammate_id,
    name,
    role,
    status,
    thread_id,
    last_error,
    created_at,
    updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(params.team_id.as_str())
        .bind(params.teammate_id.as_str())
        .bind(params.name.as_str())
        .bind(params.role.as_deref())
        .bind(params.status.as_str())
        .bind(params.thread_id.as_deref())
        .bind(params.last_error.as_deref())
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        touch_team_updated_at(&mut tx, params.team_id.as_str(), now).await?;
        tx.commit().await?;

        self.get_team_teammate(params.team_id.as_str(), params.teammate_id.as_str())
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "failed to load created teammate {} for team {}",
                    params.teammate_id,
                    params.team_id
                )
            })
    }

    pub async fn set_team_teammate_status(
        &self,
        team_id: &str,
        teammate_id: &str,
        status: crate::TeamTeammateStatus,
        thread_id: Option<&str>,
        last_error: Option<&str>,
    ) -> anyhow::Result<()> {
        let now = Utc::now().timestamp();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
UPDATE team_teammates
SET
    status = ?,
    thread_id = COALESCE(?, thread_id),
    last_error = ?,
    updated_at = ?
WHERE team_id = ? AND teammate_id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(thread_id)
        .bind(last_error)
        .bind(now)
        .bind(team_id)
        .bind(teammate_id)
        .execute(&mut *tx)
        .await?;
        touch_team_updated_at(&mut tx, team_id, now).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_team_task(
        &self,
        params: &crate::TeamTaskCreateParams,
    ) -> anyhow::Result<crate::TeamTask> {
        let now = Utc::now().timestamp();
        let completed_at = if matches!(params.status, crate::TeamTaskStatus::Completed) {
            Some(now)
        } else {
            None
        };
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
INSERT INTO team_tasks (
    team_id,
    task_id,
    title,
    description,
    status,
    assignee_teammate_id,
    created_at,
    updated_at,
    completed_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(params.team_id.as_str())
        .bind(params.task_id.as_str())
        .bind(params.title.as_str())
        .bind(params.description.as_deref())
        .bind(params.status.as_str())
        .bind(params.assignee_teammate_id.as_deref())
        .bind(now)
        .bind(now)
        .bind(completed_at)
        .execute(&mut *tx)
        .await?;
        touch_team_updated_at(&mut tx, params.team_id.as_str(), now).await?;
        tx.commit().await?;

        self.get_team_task(params.team_id.as_str(), params.task_id.as_str())
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "failed to load created task {} for team {}",
                    params.task_id,
                    params.team_id
                )
            })
    }

    pub async fn list_team_tasks(&self, team_id: &str) -> anyhow::Result<Vec<crate::TeamTask>> {
        let rows = sqlx::query_as::<_, crate::model::TeamTaskRow>(
            r#"
SELECT
    team_id,
    task_id,
    title,
    description,
    status,
    assignee_teammate_id,
    created_at,
    updated_at,
    completed_at
FROM team_tasks
WHERE team_id = ?
ORDER BY created_at ASC, task_id ASC
            "#,
        )
        .bind(team_id)
        .fetch_all(self.pool.as_ref())
        .await?;
        rows.into_iter().map(crate::TeamTask::try_from).collect()
    }

    pub async fn get_team_task(
        &self,
        team_id: &str,
        task_id: &str,
    ) -> anyhow::Result<Option<crate::TeamTask>> {
        let row = sqlx::query_as::<_, crate::model::TeamTaskRow>(
            r#"
SELECT
    team_id,
    task_id,
    title,
    description,
    status,
    assignee_teammate_id,
    created_at,
    updated_at,
    completed_at
FROM team_tasks
WHERE team_id = ? AND task_id = ?
            "#,
        )
        .bind(team_id)
        .bind(task_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(crate::TeamTask::try_from).transpose()
    }

    pub async fn update_team_task(
        &self,
        params: &crate::TeamTaskUpdateParams,
    ) -> anyhow::Result<Option<crate::TeamTask>> {
        let now = Utc::now().timestamp();
        let mut builder = QueryBuilder::<Sqlite>::new("UPDATE team_tasks SET updated_at = ");
        builder.push_bind(now);
        if let Some(title) = params.title.as_ref() {
            builder.push(", title = ");
            builder.push_bind(title);
        }
        if let Some(description) = params.description.as_ref() {
            builder.push(", description = ");
            builder.push_bind(description);
        }
        let mut next_completed_at = None;
        if let Some(status) = params.status {
            builder.push(", status = ");
            builder.push_bind(status.as_str());
            if matches!(status, crate::TeamTaskStatus::Completed) {
                next_completed_at = Some(Some(now));
            } else {
                next_completed_at = Some(None::<i64>);
            }
        }
        if let Some(completed_at) = next_completed_at {
            builder.push(", completed_at = ");
            builder.push_bind(completed_at);
        }
        if params.clear_assignee {
            builder.push(", assignee_teammate_id = NULL");
        } else if let Some(assignee_teammate_id) = params.assignee_teammate_id.as_ref() {
            builder.push(", assignee_teammate_id = ");
            builder.push_bind(assignee_teammate_id);
        }
        builder.push(" WHERE team_id = ");
        builder.push_bind(params.team_id.as_str());
        builder.push(" AND task_id = ");
        builder.push_bind(params.task_id.as_str());

        let mut tx = self.pool.begin().await?;
        let result = builder.build().execute(&mut *tx).await?;
        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(None);
        }
        touch_team_updated_at(&mut tx, params.team_id.as_str(), now).await?;
        tx.commit().await?;
        self.get_team_task(params.team_id.as_str(), params.task_id.as_str())
            .await
    }

    pub async fn create_team_mailbox_message(
        &self,
        params: &crate::TeamMailboxMessageCreateParams,
    ) -> anyhow::Result<crate::TeamMailboxMessage> {
        let now = Utc::now().timestamp();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
INSERT INTO team_mailbox_messages (
    id,
    team_id,
    sender_kind,
    sender_teammate_id,
    recipient_kind,
    recipient_teammate_id,
    body,
    created_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(params.id.as_str())
        .bind(params.team_id.as_str())
        .bind(params.sender_kind.as_str())
        .bind(params.sender_teammate_id.as_deref())
        .bind(params.recipient_kind.as_str())
        .bind(params.recipient_teammate_id.as_deref())
        .bind(params.body.as_str())
        .bind(now)
        .execute(&mut *tx)
        .await?;
        touch_team_updated_at(&mut tx, params.team_id.as_str(), now).await?;
        tx.commit().await?;

        self.get_team_mailbox_message(params.id.as_str())
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("failed to load created team mailbox message {}", params.id)
            })
    }

    pub async fn get_team_mailbox_message(
        &self,
        message_id: &str,
    ) -> anyhow::Result<Option<crate::TeamMailboxMessage>> {
        let row = sqlx::query_as::<_, crate::model::TeamMailboxMessageRow>(
            r#"
SELECT
    id,
    team_id,
    sender_kind,
    sender_teammate_id,
    recipient_kind,
    recipient_teammate_id,
    body,
    created_at
FROM team_mailbox_messages
WHERE id = ?
            "#,
        )
        .bind(message_id)
        .fetch_optional(self.pool.as_ref())
        .await?;
        row.map(crate::TeamMailboxMessage::try_from).transpose()
    }

    pub async fn list_team_mailbox_messages(
        &self,
        team_id: &str,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<crate::TeamMailboxMessage>> {
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
SELECT
    id,
    team_id,
    sender_kind,
    sender_teammate_id,
    recipient_kind,
    recipient_teammate_id,
    body,
    created_at
FROM team_mailbox_messages
WHERE team_id = 
            "#,
        );
        builder.push_bind(team_id);
        builder.push(" ORDER BY created_at DESC, id DESC");
        if let Some(limit) = limit {
            builder.push(" LIMIT ");
            builder.push_bind(limit as i64);
        }
        let mut rows: Vec<crate::model::TeamMailboxMessageRow> = builder
            .build_query_as()
            .fetch_all(self.pool.as_ref())
            .await?;
        rows.reverse();
        rows.into_iter()
            .map(crate::TeamMailboxMessage::try_from)
            .collect()
    }
}

async fn touch_team_updated_at(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    team_id: &str,
    updated_at: i64,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE teams SET updated_at = ? WHERE id = ?")
        .bind(updated_at)
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::unique_temp_dir;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn team_runtime_round_trips_entities() -> anyhow::Result<()> {
        let praxis_home = unique_temp_dir();
        let runtime = StateRuntime::init(praxis_home, "test-provider".to_string()).await?;

        let team = runtime
            .create_team(&crate::TeamCreateParams {
                id: "team_alpha".to_string(),
                lead_thread_id: "01900000-0000-7000-8000-000000000001".to_string(),
                name: "Alpha".to_string(),
                objective: Some("Ship the backend".to_string()),
                execution_mode: crate::TeamExecutionMode::ProcessFirst,
                resume_mode: crate::TeamResumeMode::Strong,
            })
            .await?;
        assert_eq!(team.name, "Alpha");

        let teammate = runtime
            .create_team_teammate(&crate::TeamTeammateCreateParams {
                team_id: team.id.clone(),
                teammate_id: "mate_backend".to_string(),
                name: "Backend".to_string(),
                role: Some("API".to_string()),
                status: crate::TeamTeammateStatus::Pending,
                thread_id: None,
                last_error: None,
            })
            .await?;
        assert_eq!(teammate.status, crate::TeamTeammateStatus::Pending);

        runtime
            .set_team_teammate_status(
                team.id.as_str(),
                teammate.teammate_id.as_str(),
                crate::TeamTeammateStatus::Active,
                Some("01900000-0000-7000-8000-000000000099"),
                None,
            )
            .await?;
        let teammate = runtime
            .get_team_teammate(team.id.as_str(), teammate.teammate_id.as_str())
            .await?
            .expect("teammate should exist");
        assert_eq!(teammate.status, crate::TeamTeammateStatus::Active);

        let task = runtime
            .create_team_task(&crate::TeamTaskCreateParams {
                team_id: team.id.clone(),
                task_id: "task_api".to_string(),
                title: "Implement team RPC".to_string(),
                description: Some("Wire the server surface".to_string()),
                status: crate::TeamTaskStatus::Pending,
                assignee_teammate_id: Some(teammate.teammate_id.clone()),
            })
            .await?;
        assert_eq!(task.status, crate::TeamTaskStatus::Pending);

        let updated_task = runtime
            .update_team_task(&crate::TeamTaskUpdateParams {
                team_id: team.id.clone(),
                task_id: task.task_id.clone(),
                title: None,
                description: None,
                status: Some(crate::TeamTaskStatus::Completed),
                assignee_teammate_id: None,
                clear_assignee: false,
            })
            .await?
            .expect("task should exist");
        assert_eq!(updated_task.status, crate::TeamTaskStatus::Completed);
        assert!(updated_task.completed_at.is_some());

        let message = runtime
            .create_team_mailbox_message(&crate::TeamMailboxMessageCreateParams {
                id: "msg_1".to_string(),
                team_id: team.id.clone(),
                sender_kind: crate::TeamMailboxParticipantKind::Lead,
                sender_teammate_id: None,
                recipient_kind: crate::TeamMailboxParticipantKind::Teammate,
                recipient_teammate_id: Some(teammate.teammate_id.clone()),
                body: "Need a status update".to_string(),
            })
            .await?;
        assert_eq!(message.body, "Need a status update");

        assert_eq!(
            runtime.list_team_teammates(team.id.as_str()).await?.len(),
            1
        );
        assert_eq!(runtime.list_team_tasks(team.id.as_str()).await?.len(), 1);
        assert_eq!(
            runtime
                .list_team_mailbox_messages(team.id.as_str(), Some(10))
                .await?
                .len(),
            1
        );

        let deleted = runtime.delete_team(team.id.as_str()).await?;
        assert_eq!(deleted, 1);
        assert!(runtime.get_team(team.id.as_str()).await?.is_none());
        assert!(
            runtime
                .list_team_teammates(team.id.as_str())
                .await?
                .is_empty()
        );
        assert!(runtime.list_team_tasks(team.id.as_str()).await?.is_empty());
        assert!(
            runtime
                .list_team_mailbox_messages(team.id.as_str(), Some(10))
                .await?
                .is_empty()
        );

        Ok(())
    }
}
