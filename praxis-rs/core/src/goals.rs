use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state_db_bridge::StateDbHandle;
use anyhow::Context;
use futures::future::BoxFuture;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ThreadGoal;
use praxis_protocol::protocol::ThreadGoalStatus;
use praxis_protocol::protocol::ThreadGoalUpdatedEvent;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::validate_thread_goal_objective;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tokio::sync::SemaphorePermit;

pub(crate) struct SetGoalRequest {
    pub(crate) objective: Option<String>,
    pub(crate) status: Option<ThreadGoalStatus>,
    pub(crate) token_budget: Option<Option<i64>>,
}

pub(crate) struct CreateGoalRequest {
    pub(crate) objective: String,
    pub(crate) token_budget: Option<i64>,
}

pub(crate) enum GoalRuntimeEvent<'a> {
    TurnStarted {
        turn_context: &'a TurnContext,
        token_usage: TokenUsage,
    },
    ToolCompletedGoal {
        turn_context: &'a TurnContext,
    },
    TurnFinished {
        turn_context: &'a TurnContext,
        turn_completed: bool,
    },
    MaybeContinueIfIdle,
    TaskAborted {
        turn_context: Option<&'a TurnContext>,
    },
}

pub(crate) struct GoalRuntimeState {
    accounting_lock: Semaphore,
    accounting: Mutex<GoalAccountingSnapshot>,
    continuation_lock: Semaphore,
    // True when the most recent goal-continuation turn produced no tool calls,
    // indicating the model stalled; further auto-continuation is suppressed
    // until new user input arrives or a non-empty continuation turn occurs.
    idle_stall_suppressed: Mutex<bool>,
    // Sub-id of the turn launched by goal continuation, if any is pending.
    // Compared at turn finish to decide whether the just-ended turn was an
    // auto-continuation (and thus subject to idle-stall suppression).
    pending_continuation_turn_id: Mutex<Option<String>>,
}

impl GoalRuntimeState {
    pub(crate) fn new() -> Self {
        Self {
            accounting_lock: Semaphore::new(1),
            accounting: Mutex::new(GoalAccountingSnapshot::new()),
            continuation_lock: Semaphore::new(1),
            idle_stall_suppressed: Mutex::new(false),
            pending_continuation_turn_id: Mutex::new(None),
        }
    }

    async fn accounting_permit(&self) -> anyhow::Result<SemaphorePermit<'_>> {
        self.accounting_lock
            .acquire()
            .await
            .context("goal accounting semaphore closed")
    }

    async fn is_continuation_suppressed(&self) -> bool {
        *self.idle_stall_suppressed.lock().await
    }

    async fn mark_continuation_suppressed(&self, suppressed: bool) {
        *self.idle_stall_suppressed.lock().await = suppressed;
    }

    async fn pending_continuation_turn_id(&self) -> Option<String> {
        self.pending_continuation_turn_id.lock().await.clone()
    }

    async fn set_pending_continuation_turn_id(&self, turn_id: Option<String>) {
        *self.pending_continuation_turn_id.lock().await = turn_id;
    }
}

#[derive(Debug)]
struct GoalAccountingSnapshot {
    turn: Option<GoalTurnAccountingSnapshot>,
    wall_clock: GoalWallClockAccountingSnapshot,
}

#[derive(Debug)]
struct GoalTurnAccountingSnapshot {
    turn_id: String,
    last_accounted_token_usage: TokenUsage,
    active_goal_id: Option<String>,
}

impl GoalAccountingSnapshot {
    fn new() -> Self {
        Self {
            turn: None,
            wall_clock: GoalWallClockAccountingSnapshot::new(),
        }
    }
}

impl GoalTurnAccountingSnapshot {
    fn new(turn_id: impl Into<String>, token_usage: TokenUsage) -> Self {
        Self {
            turn_id: turn_id.into(),
            last_accounted_token_usage: token_usage,
            active_goal_id: None,
        }
    }

    fn mark_active_goal(&mut self, goal_id: impl Into<String>) {
        self.active_goal_id = Some(goal_id.into());
    }

    fn active_goal_id(&self) -> Option<String> {
        self.active_goal_id.clone()
    }

    fn clear_active_goal(&mut self) {
        self.active_goal_id = None;
    }

    fn token_delta_since_last_accounting(&self, current: &TokenUsage) -> i64 {
        let last = &self.last_accounted_token_usage;
        let delta = TokenUsage {
            input_tokens: current.input_tokens.saturating_sub(last.input_tokens),
            cached_input_tokens: current
                .cached_input_tokens
                .saturating_sub(last.cached_input_tokens),
            cache_reported_input_tokens: current
                .cache_reported_input_tokens
                .saturating_sub(last.cache_reported_input_tokens),
            output_tokens: current.output_tokens.saturating_sub(last.output_tokens),
            reasoning_output_tokens: current
                .reasoning_output_tokens
                .saturating_sub(last.reasoning_output_tokens),
            total_tokens: current.total_tokens.saturating_sub(last.total_tokens),
        };
        goal_token_delta_for_usage(&delta)
    }

    fn mark_accounted(&mut self, current: TokenUsage) {
        self.last_accounted_token_usage = current;
    }
}

#[derive(Debug)]
struct GoalWallClockAccountingSnapshot {
    last_accounted_at: Instant,
    active_goal_id: Option<String>,
}

impl GoalWallClockAccountingSnapshot {
    fn new() -> Self {
        Self {
            last_accounted_at: Instant::now(),
            active_goal_id: None,
        }
    }

    fn time_delta_since_last_accounting(&self) -> i64 {
        i64::try_from(self.last_accounted_at.elapsed().as_secs()).unwrap_or(i64::MAX)
    }

    fn mark_accounted(&mut self, accounted_seconds: i64) {
        if accounted_seconds <= 0 {
            return;
        }
        let advance = Duration::from_secs(u64::try_from(accounted_seconds).unwrap_or(u64::MAX));
        self.last_accounted_at = self
            .last_accounted_at
            .checked_add(advance)
            .unwrap_or_else(Instant::now);
    }

    fn reset_baseline(&mut self) {
        self.last_accounted_at = Instant::now();
    }

    fn mark_active_goal(&mut self, goal_id: impl Into<String>) {
        let goal_id = goal_id.into();
        if self.active_goal_id.as_deref() != Some(goal_id.as_str()) {
            self.reset_baseline();
            self.active_goal_id = Some(goal_id);
        }
    }

    fn clear_active_goal(&mut self) {
        self.active_goal_id = None;
        self.reset_baseline();
    }
}

impl Session {
    pub(crate) fn goal_runtime_apply<'a>(
        self: &'a Arc<Self>,
        event: GoalRuntimeEvent<'a>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        match event {
            GoalRuntimeEvent::TurnStarted {
                turn_context,
                token_usage,
            } => Box::pin(async move {
                self.mark_thread_goal_turn_started(turn_context, token_usage)
                    .await;
                Ok(())
            }),
            GoalRuntimeEvent::ToolCompletedGoal { turn_context } => Box::pin(async move {
                self.account_thread_goal_progress(
                    turn_context,
                    praxis_state::GoalAccountingMode::ActiveOrComplete,
                )
                .await?;
                Ok(())
            }),
            GoalRuntimeEvent::TurnFinished {
                turn_context,
                turn_completed,
            } => Box::pin(async move {
                self.finish_thread_goal_turn(turn_context, turn_completed)
                    .await;
                Ok(())
            }),
            GoalRuntimeEvent::MaybeContinueIfIdle => Box::pin(async move {
                self.maybe_continue_goal_if_idle_runtime().await;
                Ok(())
            }),
            GoalRuntimeEvent::TaskAborted { turn_context } => Box::pin(async move {
                self.handle_thread_goal_task_abort(turn_context).await;
                Ok(())
            }),
        }
    }

    pub(crate) async fn get_thread_goal(&self) -> anyhow::Result<Option<ThreadGoal>> {
        let state_db = self.require_state_db_for_thread_goals().await?;
        state_db
            .get_thread_goal(self.conversation_id)
            .await
            .map(|goal| goal.map(protocol_goal_from_state))
    }

    pub(crate) async fn user_set_thread_goal(
        self: &Arc<Self>,
        objective: String,
        token_budget: Option<Option<i64>>,
    ) -> anyhow::Result<ThreadGoal> {
        validate_goal_budget(token_budget.flatten())?;
        let objective = objective.trim().to_string();
        if let Err(err) = validate_thread_goal_objective(objective.as_str()) {
            anyhow::bail!("{err}");
        }

        let state_db = self.require_state_db_for_thread_goals().await?;
        self.account_thread_goal_wall_clock_usage(
            &state_db,
            praxis_state::GoalAccountingMode::ActiveOnly,
        )
        .await?;
        let existing_goal = state_db.get_thread_goal(self.conversation_id).await?;
        let goal = if let Some(existing_goal) = existing_goal {
            state_db
                .update_thread_goal(
                    self.conversation_id,
                    praxis_state::GoalUpdate {
                        objective: Some(objective),
                        status: Some(praxis_state::ThreadGoalStatus::Active),
                        token_budget,
                        expected_goal_id: Some(existing_goal.goal_id),
                    },
                )
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "cannot update goal for thread {}: no goal exists",
                        self.conversation_id
                    )
                })?
        } else {
            state_db
                .insert_thread_goal(
                    self.conversation_id,
                    objective.as_str(),
                    praxis_state::ThreadGoalStatus::Active,
                    token_budget.flatten(),
                )
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "thread {} already has a goal; update or clear it before creating another",
                        self.conversation_id
                    )
                })?
        };
        state_db
            .delete_thread_heartbeat(self.conversation_id)
            .await?;
        let goal_id = goal.goal_id.clone();
        let protocol_goal = protocol_goal_from_state(goal);
        self.mark_active_goal_accounting(
            goal_id,
            None,
            self.total_token_usage().await.unwrap_or_default(),
        )
        .await;
        Ok(protocol_goal)
    }

    pub(crate) async fn create_thread_goal(
        self: &Arc<Self>,
        turn_context: &TurnContext,
        request: CreateGoalRequest,
    ) -> anyhow::Result<ThreadGoal> {
        validate_goal_budget(request.token_budget)?;
        let objective = request.objective.trim().to_string();
        if let Err(err) = validate_thread_goal_objective(objective.as_str()) {
            anyhow::bail!("{err}");
        }

        let state_db = self.require_state_db_for_thread_goals().await?;
        self.account_thread_goal_wall_clock_usage(
            &state_db,
            praxis_state::GoalAccountingMode::ActiveOnly,
        )
        .await?;
        let goal = state_db
            .insert_thread_goal(
                self.conversation_id,
                objective.as_str(),
                praxis_state::ThreadGoalStatus::Active,
                request.token_budget,
            )
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "thread {} already has a goal; complete or clear it before creating another",
                    self.conversation_id
                )
            })?;
        state_db
            .delete_thread_heartbeat(self.conversation_id)
            .await?;
        let goal_id = goal.goal_id.clone();
        let protocol_goal = protocol_goal_from_state(goal);
        self.mark_active_goal_accounting(
            goal_id,
            Some(turn_context.sub_id.clone()),
            self.total_token_usage().await.unwrap_or_default(),
        )
        .await;
        self.emit_thread_goal_updated(turn_context, protocol_goal.clone())
            .await;
        Ok(protocol_goal)
    }

    pub(crate) async fn set_thread_goal(
        self: &Arc<Self>,
        turn_context: &TurnContext,
        request: SetGoalRequest,
    ) -> anyhow::Result<ThreadGoal> {
        let protocol_goal = self
            .set_thread_goal_without_event(request, Some(turn_context.sub_id.clone()))
            .await?;
        self.emit_thread_goal_updated(turn_context, protocol_goal.clone())
            .await;
        Ok(protocol_goal)
    }

    pub(crate) async fn user_update_thread_goal(
        self: &Arc<Self>,
        request: SetGoalRequest,
    ) -> anyhow::Result<ThreadGoal> {
        self.set_thread_goal_without_event(request, None).await
    }

    pub(crate) async fn user_clear_thread_goal(self: &Arc<Self>) -> anyhow::Result<bool> {
        let state_db = self.require_state_db_for_thread_goals().await?;
        self.account_thread_goal_wall_clock_usage(
            &state_db,
            praxis_state::GoalAccountingMode::ActiveOnly,
        )
        .await?;
        let cleared = state_db.delete_thread_goal(self.conversation_id).await?;
        if cleared {
            self.clear_stopped_thread_goal_runtime_state().await;
        }
        Ok(cleared)
    }

    async fn set_thread_goal_without_event(
        self: &Arc<Self>,
        request: SetGoalRequest,
        turn_id: Option<String>,
    ) -> anyhow::Result<ThreadGoal> {
        let SetGoalRequest {
            objective,
            status,
            token_budget,
        } = request;
        validate_goal_budget(token_budget.flatten())?;
        let objective = objective.map(|objective| objective.trim().to_string());
        if let Some(objective) = objective.as_deref()
            && let Err(err) = validate_thread_goal_objective(objective)
        {
            anyhow::bail!("{err}");
        }

        let state_db = self.require_state_db_for_thread_goals().await?;
        self.account_thread_goal_wall_clock_usage(
            &state_db,
            praxis_state::GoalAccountingMode::ActiveOnly,
        )
        .await?;
        let existing_goal = state_db.get_thread_goal(self.conversation_id).await?;
        let expected_goal_id = existing_goal.map(|goal| goal.goal_id);
        let goal = state_db
            .update_thread_goal(
                self.conversation_id,
                praxis_state::GoalUpdate {
                    objective,
                    status: status.map(state_goal_status_from_protocol),
                    token_budget,
                    expected_goal_id,
                },
            )
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "cannot update goal for thread {}: no goal exists",
                    self.conversation_id
                )
            })?;
        state_db
            .delete_thread_heartbeat(self.conversation_id)
            .await?;
        let goal_id = goal.goal_id.clone();
        let goal_status = goal.status;
        let protocol_goal = protocol_goal_from_state(goal);
        if goal_status == praxis_state::ThreadGoalStatus::Active {
            self.mark_active_goal_accounting(
                goal_id,
                turn_id,
                self.total_token_usage().await.unwrap_or_default(),
            )
            .await;
        } else {
            self.clear_stopped_thread_goal_runtime_state().await;
        }
        Ok(protocol_goal)
    }

    async fn mark_thread_goal_turn_started(
        self: &Arc<Self>,
        turn_context: &TurnContext,
        token_usage: TokenUsage,
    ) {
        if should_ignore_goal_for_mode(turn_context.collaboration_mode.mode) {
            return;
        }
        // If this turn was not launched by goal continuation, treat it as user-
        // or system-driven input that should clear any prior idle stall and let
        // continuation resume once the goal goes idle again.
        let is_continuation_turn = self
            .goal_runtime
            .pending_continuation_turn_id()
            .await
            .is_some_and(|pending| pending == turn_context.sub_id);
        if !is_continuation_turn && self.goal_runtime.is_continuation_suppressed().await {
            self.goal_runtime.mark_continuation_suppressed(false).await;
        }
        let state_db = match self.state_db_for_thread_goals().await {
            Ok(Some(state_db)) => state_db,
            Ok(None) => return,
            Err(err) => {
                tracing::warn!("failed to open state db for thread goal turn start: {err:#}");
                return;
            }
        };
        let goal = match state_db.get_thread_goal(self.conversation_id).await {
            Ok(Some(goal)) if goal.status == praxis_state::ThreadGoalStatus::Active => goal,
            Ok(_) => {
                self.clear_stopped_thread_goal_runtime_state().await;
                return;
            }
            Err(err) => {
                tracing::warn!("failed to read thread goal at turn start: {err:#}");
                return;
            }
        };
        self.mark_active_goal_accounting(
            goal.goal_id,
            Some(turn_context.sub_id.clone()),
            token_usage,
        )
        .await;
    }

    async fn mark_active_goal_accounting(
        &self,
        goal_id: String,
        turn_id: Option<String>,
        token_usage: TokenUsage,
    ) {
        let mut accounting = self.goal_runtime.accounting.lock().await;
        if let Some(turn_id) = turn_id {
            let mut turn = GoalTurnAccountingSnapshot::new(turn_id, token_usage);
            turn.mark_active_goal(goal_id.clone());
            accounting.turn = Some(turn);
        }
        accounting.wall_clock.mark_active_goal(goal_id);
    }

    async fn finish_thread_goal_turn(
        self: &Arc<Self>,
        turn_context: &TurnContext,
        turn_completed: bool,
    ) {
        let mode = if turn_completed {
            praxis_state::GoalAccountingMode::ActiveOrComplete
        } else {
            praxis_state::GoalAccountingMode::ActiveOrStopped
        };
        if let Err(err) = self.account_thread_goal_progress(turn_context, mode).await {
            tracing::warn!("failed to account thread goal progress at turn finish: {err:#}");
        }
        self.update_continuation_stall_state(turn_context).await;
        let mut accounting = self.goal_runtime.accounting.lock().await;
        accounting.turn = None;
    }

    // Decide whether goal continuation should be suppressed after this turn.
    // A continuation turn that produced no tool calls is treated as an idle
    // stall: continuation is suppressed until new user input clears it. A
    // continuation turn that did call tools clears any prior stall. Non-
    // continuation turns are left alone here; user input clears the stall flag
    // when a new user-driven turn starts.
    async fn update_continuation_stall_state(self: &Arc<Self>, turn_context: &TurnContext) {
        let Some(pending) = self.goal_runtime.pending_continuation_turn_id().await else {
            return;
        };
        // This was a goal-continuation turn only if the finished turn matches.
        if pending != turn_context.sub_id {
            return;
        }
        self.goal_runtime
            .set_pending_continuation_turn_id(None)
            .await;
        let stalled = !turn_context.tool_loop_guard.has_any_tool_call();
        self.goal_runtime
            .mark_continuation_suppressed(stalled)
            .await;
        if stalled {
            tracing::info!(
                turn_id = %turn_context.sub_id,
                "goal continuation turn produced no tool calls; suppressing further auto-continuation until new user input"
            );
        }
    }

    async fn handle_thread_goal_task_abort(self: &Arc<Self>, turn_context: Option<&TurnContext>) {
        if let Some(turn_context) = turn_context
            && let Err(err) = self
                .account_thread_goal_progress(
                    turn_context,
                    praxis_state::GoalAccountingMode::ActiveOrStopped,
                )
                .await
        {
            tracing::warn!("failed to account thread goal progress at abort: {err:#}");
        }
        // An abort is not a normal finish; clear any pending continuation tag so
        // the next user-driven turn starts clean and is not mistaken for a stall.
        self.goal_runtime
            .set_pending_continuation_turn_id(None)
            .await;
        let mut accounting = self.goal_runtime.accounting.lock().await;
        accounting.turn = None;
    }

    async fn account_thread_goal_progress(
        self: &Arc<Self>,
        turn_context: &TurnContext,
        mode: praxis_state::GoalAccountingMode,
    ) -> anyhow::Result<()> {
        let _permit = self.goal_runtime.accounting_permit().await?;
        let state_db = match self.state_db_for_thread_goals().await? {
            Some(state_db) => state_db,
            None => return Ok(()),
        };
        let current_token_usage = self.total_token_usage().await.unwrap_or_default();
        let (time_delta_seconds, token_delta, expected_goal_id) = {
            let accounting = self.goal_runtime.accounting.lock().await;
            let Some(turn) = accounting.turn.as_ref() else {
                return Ok(());
            };
            if turn.turn_id != turn_context.sub_id {
                return Ok(());
            }
            (
                accounting.wall_clock.time_delta_since_last_accounting(),
                turn.token_delta_since_last_accounting(&current_token_usage),
                turn.active_goal_id(),
            )
        };

        if time_delta_seconds == 0 && token_delta == 0 {
            return Ok(());
        }
        let outcome = state_db
            .account_thread_goal_usage(
                self.conversation_id,
                time_delta_seconds,
                token_delta,
                mode,
                expected_goal_id.as_deref(),
            )
            .await?;

        {
            let mut accounting = self.goal_runtime.accounting.lock().await;
            if let Some(turn) = accounting.turn.as_mut()
                && turn.turn_id == turn_context.sub_id
            {
                turn.mark_accounted(current_token_usage);
                if matches!(&outcome, praxis_state::GoalAccountingOutcome::Updated(_)) {
                    accounting.wall_clock.mark_accounted(time_delta_seconds);
                }
            }
        }

        if let praxis_state::GoalAccountingOutcome::Updated(goal) = outcome {
            let protocol_goal = protocol_goal_from_state(goal);
            if protocol_goal.status != ThreadGoalStatus::Active {
                self.clear_stopped_thread_goal_runtime_state().await;
            }
            self.emit_thread_goal_updated(turn_context, protocol_goal)
                .await;
        }
        Ok(())
    }

    async fn account_thread_goal_wall_clock_usage(
        &self,
        state_db: &StateDbHandle,
        mode: praxis_state::GoalAccountingMode,
    ) -> anyhow::Result<()> {
        let _permit = self.goal_runtime.accounting_permit().await?;
        let (time_delta_seconds, expected_goal_id) = {
            let accounting = self.goal_runtime.accounting.lock().await;
            (
                accounting.wall_clock.time_delta_since_last_accounting(),
                accounting.wall_clock.active_goal_id.clone(),
            )
        };
        if time_delta_seconds <= 0 {
            return Ok(());
        }
        let outcome = state_db
            .account_thread_goal_usage(
                self.conversation_id,
                time_delta_seconds,
                0,
                mode,
                expected_goal_id.as_deref(),
            )
            .await?;
        {
            let mut accounting = self.goal_runtime.accounting.lock().await;
            if matches!(&outcome, praxis_state::GoalAccountingOutcome::Updated(_)) {
                accounting.wall_clock.mark_accounted(time_delta_seconds);
            }
        }
        Ok(())
    }

    async fn clear_stopped_thread_goal_runtime_state(&self) {
        let mut accounting = self.goal_runtime.accounting.lock().await;
        if let Some(turn) = accounting.turn.as_mut() {
            turn.clear_active_goal();
        }
        accounting.wall_clock.clear_active_goal();
    }

    async fn maybe_continue_goal_if_idle_runtime(self: &Arc<Self>) {
        let Ok(_permit) = self.goal_runtime.continuation_lock.try_acquire() else {
            return;
        };
        if self.goal_runtime.is_continuation_suppressed().await
            || self.active_turn.lock().await.is_some()
            || self.has_pending_work_for_idle_turn().await
            || should_ignore_goal_for_mode(self.collaboration_mode().await.mode)
        {
            return;
        }
        let state_db = match self.state_db_for_thread_goals().await {
            Ok(Some(state_db)) => state_db,
            Ok(None) => return,
            Err(err) => {
                tracing::warn!("failed to open state db for goal continuation: {err:#}");
                return;
            }
        };
        let goal = match state_db.get_thread_goal(self.conversation_id).await {
            Ok(Some(goal)) if goal.status == praxis_state::ThreadGoalStatus::Active => goal,
            Ok(_) => return,
            Err(err) => {
                tracing::warn!("failed to read thread goal for continuation: {err:#}");
                return;
            }
        };
        let item = goal_context_input_item(continuation_prompt(&protocol_goal_from_state(goal)));
        self.queue_response_items_for_next_turn(vec![item]).await;
        // Tag the upcoming turn as a goal continuation so its tool-call activity
        // can be checked at turn finish to suppress idle stalls.
        let continuation_turn_id = uuid::Uuid::new_v4().to_string();
        self.goal_runtime
            .set_pending_continuation_turn_id(Some(continuation_turn_id.clone()))
            .await;
        self.maybe_start_turn_for_pending_work_with_sub_id(continuation_turn_id)
            .await;
    }

    async fn emit_thread_goal_updated(&self, turn_context: &TurnContext, goal: ThreadGoal) {
        self.send_event(
            turn_context,
            EventMsg::ThreadGoalUpdated(ThreadGoalUpdatedEvent {
                thread_id: self.conversation_id,
                turn_id: Some(turn_context.sub_id.clone()),
                goal,
            }),
        )
        .await;
    }

    async fn state_db_for_thread_goals(&self) -> anyhow::Result<Option<StateDbHandle>> {
        self.state_db_for_thread_feature("thread goals").await
    }

    async fn require_state_db_for_thread_goals(&self) -> anyhow::Result<StateDbHandle> {
        self.require_state_db_for_thread_feature("thread goals")
            .await
    }
}

fn should_ignore_goal_for_mode(mode: ModeKind) -> bool {
    mode == ModeKind::Plan
}

fn continuation_prompt(goal: &ThreadGoal) -> String {
    let token_budget = goal
        .token_budget
        .map(|budget| budget.to_string())
        .unwrap_or_else(|| "none".to_string());
    let remaining_tokens = goal
        .token_budget
        .map(|budget| (budget - goal.tokens_used).max(0).to_string())
        .unwrap_or_else(|| "unbounded".to_string());
    let objective = escape_xml_text(&goal.objective);
    format!(
        r#"<praxis_internal_context source="goal">
The thread has an active persisted goal. Continue working toward it without waiting for user input.

<objective>
{objective}
</objective>

Tokens used: {tokens_used}
Token budget: {token_budget}
Tokens remaining: {remaining_tokens}

Call update_goal with status "complete" only when the objective is achieved and no required work remains.
Call update_goal with status "blocked" only after the same blocking condition has repeated for at least three consecutive goal turns and you are truly at an impasse.
</praxis_internal_context>"#,
        tokens_used = goal.tokens_used,
    )
}

fn goal_context_input_item(prompt: String) -> ResponseInputItem {
    ResponseInputItem::Message {
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: prompt }],
    }
}

fn escape_xml_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn protocol_goal_from_state(goal: praxis_state::ThreadGoal) -> ThreadGoal {
    ThreadGoal {
        thread_id: goal.thread_id,
        objective: goal.objective,
        status: protocol_goal_status_from_state(goal.status),
        token_budget: goal.token_budget,
        tokens_used: goal.tokens_used,
        time_used_seconds: goal.time_used_seconds,
        created_at: goal.created_at.timestamp(),
        updated_at: goal.updated_at.timestamp(),
    }
}

fn protocol_goal_status_from_state(status: praxis_state::ThreadGoalStatus) -> ThreadGoalStatus {
    match status {
        praxis_state::ThreadGoalStatus::Active => ThreadGoalStatus::Active,
        praxis_state::ThreadGoalStatus::Paused => ThreadGoalStatus::Paused,
        praxis_state::ThreadGoalStatus::Blocked => ThreadGoalStatus::Blocked,
        praxis_state::ThreadGoalStatus::UsageLimited => ThreadGoalStatus::UsageLimited,
        praxis_state::ThreadGoalStatus::BudgetLimited => ThreadGoalStatus::BudgetLimited,
        praxis_state::ThreadGoalStatus::Complete => ThreadGoalStatus::Complete,
    }
}

fn state_goal_status_from_protocol(status: ThreadGoalStatus) -> praxis_state::ThreadGoalStatus {
    match status {
        ThreadGoalStatus::Active => praxis_state::ThreadGoalStatus::Active,
        ThreadGoalStatus::Paused => praxis_state::ThreadGoalStatus::Paused,
        ThreadGoalStatus::Blocked => praxis_state::ThreadGoalStatus::Blocked,
        ThreadGoalStatus::UsageLimited => praxis_state::ThreadGoalStatus::UsageLimited,
        ThreadGoalStatus::BudgetLimited => praxis_state::ThreadGoalStatus::BudgetLimited,
        ThreadGoalStatus::Complete => praxis_state::ThreadGoalStatus::Complete,
    }
}

fn validate_goal_budget(value: Option<i64>) -> anyhow::Result<()> {
    if let Some(value) = value
        && value <= 0
    {
        anyhow::bail!("goal budgets must be positive when provided");
    }
    Ok(())
}

fn goal_token_delta_for_usage(usage: &TokenUsage) -> i64 {
    usage
        .non_cached_input()
        .saturating_add(usage.output_tokens.max(0))
}
