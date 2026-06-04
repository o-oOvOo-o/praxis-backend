use super::*;
use crate::model::{ThreadGoalRow, datetime_to_millis};
use uuid::Uuid;

pub struct GoalUpdate {
    pub objective: Option<String>,
    pub status: Option<crate::ThreadGoalStatus>,
    pub token_budget: Option<Option<i64>>,
    pub expected_goal_id: Option<String>,
}

pub enum GoalAccountingOutcome {
    Unchanged(Option<crate::ThreadGoal>),
    Updated(crate::ThreadGoal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalAccountingMode {
    ActiveStatusOnly,
    ActiveOnly,
    ActiveOrComplete,
    ActiveOrStopped,
}

impl StateRuntime {
    pub async fn get_thread_goal(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Option<crate::ThreadGoal>> {
        let row = sqlx::query(
            r#"
SELECT
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
FROM thread_goals
WHERE thread_id = ?
            "#,
        )
        .bind(thread_id.to_string())
        .fetch_optional(self.pool.as_ref())
        .await?;

        row.map(|row| thread_goal_from_row(&row)).transpose()
    }

    pub async fn replace_thread_goal(
        &self,
        thread_id: ThreadId,
        objective: &str,
        status: crate::ThreadGoalStatus,
        token_budget: Option<i64>,
    ) -> anyhow::Result<crate::ThreadGoal> {
        let goal_id = Uuid::new_v4().to_string();
        let now = datetime_to_millis(Utc::now());
        let status = status_after_budget_limit(status, 0, token_budget);
        let row = sqlx::query(
            r#"
INSERT INTO thread_goals (
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, ?, ?, 0, 0, ?, ?)
ON CONFLICT(thread_id) DO UPDATE SET
    goal_id = excluded.goal_id,
    objective = excluded.objective,
    status = excluded.status,
    token_budget = excluded.token_budget,
    tokens_used = 0,
    time_used_seconds = 0,
    created_at_ms = excluded.created_at_ms,
    updated_at_ms = excluded.updated_at_ms
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
            "#,
        )
        .bind(thread_id.to_string())
        .bind(goal_id)
        .bind(objective)
        .bind(status.as_str())
        .bind(token_budget)
        .bind(now)
        .bind(now)
        .fetch_one(self.pool.as_ref())
        .await?;

        thread_goal_from_row(&row)
    }

    pub async fn insert_thread_goal(
        &self,
        thread_id: ThreadId,
        objective: &str,
        status: crate::ThreadGoalStatus,
        token_budget: Option<i64>,
    ) -> anyhow::Result<Option<crate::ThreadGoal>> {
        let goal_id = Uuid::new_v4().to_string();
        let now = datetime_to_millis(Utc::now());
        let status = status_after_budget_limit(status, 0, token_budget);
        let row = sqlx::query(
            r#"
INSERT INTO thread_goals (
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
) VALUES (?, ?, ?, ?, ?, 0, 0, ?, ?)
ON CONFLICT(thread_id) DO NOTHING
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
            "#,
        )
        .bind(thread_id.to_string())
        .bind(goal_id)
        .bind(objective)
        .bind(status.as_str())
        .bind(token_budget)
        .bind(now)
        .bind(now)
        .fetch_optional(self.pool.as_ref())
        .await?;

        row.map(|row| thread_goal_from_row(&row)).transpose()
    }

    pub async fn update_thread_goal(
        &self,
        thread_id: ThreadId,
        update: GoalUpdate,
    ) -> anyhow::Result<Option<crate::ThreadGoal>> {
        let GoalUpdate {
            objective,
            status,
            token_budget,
            expected_goal_id,
        } = update;
        let objective = objective.as_deref();
        let expected_goal_id = expected_goal_id.as_deref();
        let now = datetime_to_millis(Utc::now());
        let result = match (status, token_budget) {
            (Some(status), Some(token_budget)) => {
                sqlx::query(
                    r#"
UPDATE thread_goals
SET
    objective = COALESCE(?, objective),
    status = CASE
        WHEN status = ? AND ? IN (?, ?) THEN status
        WHEN ? = 'active' AND ? IS NOT NULL AND tokens_used >= ? THEN ?
        ELSE ?
    END,
    token_budget = ?,
    updated_at_ms = ?
WHERE thread_id = ?
  AND (? IS NULL OR goal_id = ?)
            "#,
                )
                .bind(objective)
                .bind(crate::ThreadGoalStatus::BudgetLimited.as_str())
                .bind(status.as_str())
                .bind(crate::ThreadGoalStatus::Paused.as_str())
                .bind(crate::ThreadGoalStatus::Blocked.as_str())
                .bind(status.as_str())
                .bind(token_budget)
                .bind(token_budget)
                .bind(crate::ThreadGoalStatus::BudgetLimited.as_str())
                .bind(status.as_str())
                .bind(token_budget)
                .bind(now)
                .bind(thread_id.to_string())
                .bind(expected_goal_id)
                .bind(expected_goal_id)
                .execute(self.pool.as_ref())
                .await?
            }
            (Some(status), None) => {
                sqlx::query(
                    r#"
UPDATE thread_goals
SET
    objective = COALESCE(?, objective),
    status = CASE
        WHEN status = ? AND ? IN (?, ?) THEN status
        WHEN ? = 'active' AND token_budget IS NOT NULL AND tokens_used >= token_budget THEN ?
        ELSE ?
    END,
    updated_at_ms = ?
WHERE thread_id = ?
  AND (? IS NULL OR goal_id = ?)
            "#,
                )
                .bind(objective)
                .bind(crate::ThreadGoalStatus::BudgetLimited.as_str())
                .bind(status.as_str())
                .bind(crate::ThreadGoalStatus::Paused.as_str())
                .bind(crate::ThreadGoalStatus::Blocked.as_str())
                .bind(status.as_str())
                .bind(crate::ThreadGoalStatus::BudgetLimited.as_str())
                .bind(status.as_str())
                .bind(now)
                .bind(thread_id.to_string())
                .bind(expected_goal_id)
                .bind(expected_goal_id)
                .execute(self.pool.as_ref())
                .await?
            }
            (None, Some(token_budget)) => {
                sqlx::query(
                    r#"
UPDATE thread_goals
SET
    objective = COALESCE(?, objective),
    token_budget = ?,
    status = CASE
        WHEN status = 'active' AND ? IS NOT NULL AND tokens_used >= ? THEN ?
        ELSE status
    END,
    updated_at_ms = ?
WHERE thread_id = ?
  AND (? IS NULL OR goal_id = ?)
            "#,
                )
                .bind(objective)
                .bind(token_budget)
                .bind(token_budget)
                .bind(token_budget)
                .bind(crate::ThreadGoalStatus::BudgetLimited.as_str())
                .bind(now)
                .bind(thread_id.to_string())
                .bind(expected_goal_id)
                .bind(expected_goal_id)
                .execute(self.pool.as_ref())
                .await?
            }
            (None, None) => {
                if let Some(objective) = objective {
                    sqlx::query(
                        r#"
UPDATE thread_goals
SET
    objective = ?,
    updated_at_ms = ?
WHERE thread_id = ?
  AND (? IS NULL OR goal_id = ?)
            "#,
                    )
                    .bind(objective)
                    .bind(now)
                    .bind(thread_id.to_string())
                    .bind(expected_goal_id)
                    .bind(expected_goal_id)
                    .execute(self.pool.as_ref())
                    .await?
                } else {
                    let goal = self.get_thread_goal(thread_id).await?;
                    return Ok(match (goal, expected_goal_id) {
                        (Some(goal), Some(expected_goal_id))
                            if goal.goal_id != expected_goal_id =>
                        {
                            None
                        }
                        (goal, _) => goal,
                    });
                }
            }
        };

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_thread_goal(thread_id).await
    }

    pub async fn pause_active_thread_goal(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Option<crate::ThreadGoal>> {
        self.update_active_thread_goal_status(thread_id, crate::ThreadGoalStatus::Paused)
            .await
    }

    pub async fn usage_limit_active_thread_goal(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Option<crate::ThreadGoal>> {
        self.update_active_thread_goal_status(thread_id, crate::ThreadGoalStatus::UsageLimited)
            .await
    }

    async fn update_active_thread_goal_status(
        &self,
        thread_id: ThreadId,
        status: crate::ThreadGoalStatus,
    ) -> anyhow::Result<Option<crate::ThreadGoal>> {
        let now = datetime_to_millis(Utc::now());
        let result = sqlx::query(
            r#"
UPDATE thread_goals
SET
    status = ?,
    updated_at_ms = ?
WHERE thread_id = ?
  AND (
      status = 'active'
      OR (
          ? = 'usage_limited'
          AND status = 'budget_limited'
      )
  )
            "#,
        )
        .bind(status.as_str())
        .bind(now)
        .bind(thread_id.to_string())
        .bind(status.as_str())
        .execute(self.pool.as_ref())
        .await?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get_thread_goal(thread_id).await
    }

    pub async fn delete_thread_goal(&self, thread_id: ThreadId) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"
DELETE FROM thread_goals
WHERE thread_id = ?
            "#,
        )
        .bind(thread_id.to_string())
        .execute(self.pool.as_ref())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn account_thread_goal_usage(
        &self,
        thread_id: ThreadId,
        time_delta_seconds: i64,
        token_delta: i64,
        mode: GoalAccountingMode,
        expected_goal_id: Option<&str>,
    ) -> anyhow::Result<GoalAccountingOutcome> {
        let time_delta_seconds = time_delta_seconds.max(0);
        let token_delta = token_delta.max(0);
        if time_delta_seconds == 0 && token_delta == 0 {
            return Ok(GoalAccountingOutcome::Unchanged(
                self.get_thread_goal(thread_id).await?,
            ));
        }

        let now = datetime_to_millis(Utc::now());
        let active_or_stopped_status_filter =
            "status IN ('active', 'paused', 'blocked', 'usage_limited', 'budget_limited')";
        let status_filter = match mode {
            GoalAccountingMode::ActiveStatusOnly => "status = 'active'",
            GoalAccountingMode::ActiveOnly => "status IN ('active', 'budget_limited')",
            GoalAccountingMode::ActiveOrComplete => {
                "status IN ('active', 'budget_limited', 'complete')"
            }
            GoalAccountingMode::ActiveOrStopped => active_or_stopped_status_filter,
        };
        let budget_limit_status_filter = match mode {
            GoalAccountingMode::ActiveStatusOnly
            | GoalAccountingMode::ActiveOnly
            | GoalAccountingMode::ActiveOrComplete => "status = 'active'",
            GoalAccountingMode::ActiveOrStopped => active_or_stopped_status_filter,
        };
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
UPDATE thread_goals
SET
    time_used_seconds = time_used_seconds +
            "#,
        );
        builder.push_bind(time_delta_seconds);
        builder.push(
            r#",
    tokens_used = tokens_used +
            "#,
        );
        builder.push_bind(token_delta);
        builder.push(
            r#",
    status = CASE
        WHEN
            "#,
        );
        builder.push(budget_limit_status_filter);
        builder.push(
            r#"
            AND token_budget IS NOT NULL
            AND tokens_used +
            "#,
        );
        builder.push_bind(token_delta);
        builder.push(
            r#"
                >= token_budget
            THEN
            "#,
        );
        builder.push_bind(crate::ThreadGoalStatus::BudgetLimited.as_str());
        builder.push(
            r#"
        ELSE status
    END,
    updated_at_ms =
            "#,
        );
        builder.push_bind(now);
        builder.push(
            r#"
WHERE thread_id =
            "#,
        );
        builder.push_bind(thread_id.to_string());
        builder.push(" AND ");
        builder.push(status_filter);
        if let Some(expected_goal_id) = expected_goal_id {
            builder.push(" AND goal_id = ").push_bind(expected_goal_id);
        }
        builder.push(
            r#"
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
            "#,
        );

        let row = builder.build().fetch_optional(self.pool.as_ref()).await?;

        let Some(row) = row else {
            return Ok(GoalAccountingOutcome::Unchanged(
                self.get_thread_goal(thread_id).await?,
            ));
        };

        let updated = thread_goal_from_row(&row)?;
        Ok(GoalAccountingOutcome::Updated(updated))
    }
}

fn thread_goal_from_row(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<crate::ThreadGoal> {
    ThreadGoalRow::try_from_row(row).and_then(crate::ThreadGoal::try_from)
}

fn status_after_budget_limit(
    status: crate::ThreadGoalStatus,
    tokens_used: i64,
    token_budget: Option<i64>,
) -> crate::ThreadGoalStatus {
    if status == crate::ThreadGoalStatus::Active
        && token_budget.is_some_and(|budget| tokens_used >= budget)
    {
        crate::ThreadGoalStatus::BudgetLimited
    } else {
        status
    }
}
