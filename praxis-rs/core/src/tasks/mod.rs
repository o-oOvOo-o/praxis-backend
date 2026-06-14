mod compact;
mod ghost_snapshot;
mod regular;
mod review;
mod undo;
mod user_shell;

use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use async_trait::async_trait;
use tokio::select;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing::info_span;
use tracing::trace;
use tracing::warn;

use crate::contextual_user_message::TURN_ABORTED_CLOSE_TAG;
use crate::contextual_user_message::TURN_ABORTED_OPEN_TAG;
use crate::goals::GoalRuntimeEvent;
use crate::hook_runtime::record_pending_inputs;
use crate::models_manager::manager::ModelsManager;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::ActiveTurn;
use crate::state::AgentTaskKind;
use crate::state::RunningAgentTask;
use crate::stream_events_utils::emit_synthetic_final_answer;
use crate::stream_events_utils::synthetic_final_item_for_guard;
use praxis_login::AuthManager;
use praxis_otel::SessionTelemetry;
use praxis_otel::metrics::names::TURN_E2E_DURATION_METRIC;
use praxis_otel::metrics::names::TURN_NETWORK_PROXY_METRIC;
use praxis_otel::metrics::names::TURN_TOKEN_USAGE_METRIC;
use praxis_otel::metrics::names::TURN_TOOL_CALL_METRIC;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnAbortedEvent;
use praxis_protocol::protocol::TurnCompleteEvent;
use praxis_protocol::user_input::UserInput;

pub(crate) use compact::CompactTask;
pub(crate) use ghost_snapshot::GhostSnapshotTask;
use praxis_features::Feature;
pub(crate) use regular::RegularAgentTask;
pub(crate) use review::ReviewTask;
pub(crate) use undo::UndoTask;
pub(crate) use user_shell::UserShellCommandMode;
pub(crate) use user_shell::UserShellCommandTask;
pub(crate) use user_shell::execute_user_shell_command;

const GRACEFULL_INTERRUPTION_TIMEOUT_MS: u64 = 100;
const TURN_ABORTED_INTERRUPTED_GUIDANCE: &str = "The user interrupted the previous turn on purpose. Any running unified exec processes may still be running in the background. If any tools/commands were aborted, they may have partially executed.";

/// Shared model-visible marker used by both the real interrupt path and
/// interrupted fork snapshots.
pub(crate) fn interrupted_turn_history_marker() -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "{TURN_ABORTED_OPEN_TAG}\n{TURN_ABORTED_INTERRUPTED_GUIDANCE}\n{TURN_ABORTED_CLOSE_TAG}"
            ),
        }],
        end_turn: None,
        phase: None,
    }
}

fn emit_turn_network_proxy_metric(
    session_telemetry: &SessionTelemetry,
    network_proxy_active: bool,
    tmp_mem: (&str, &str),
) {
    let active = if network_proxy_active {
        "true"
    } else {
        "false"
    };
    session_telemetry.counter(
        TURN_NETWORK_PROXY_METRIC,
        /*inc*/ 1,
        &[("active", active), tmp_mem],
    );
}

fn emit_turn_token_usage_metrics(
    session_telemetry: &SessionTelemetry,
    usage: &TokenUsage,
    tmp_mem: (&str, &str),
) {
    for (token_type, value) in [
        ("total", usage.total_tokens),
        ("input", usage.input_tokens),
        ("cached_input", usage.cached_input()),
        ("output", usage.output_tokens),
        ("reasoning_output", usage.reasoning_output_tokens),
    ] {
        session_telemetry.histogram(
            TURN_TOKEN_USAGE_METRIC,
            value,
            &[("token_type", token_type), tmp_mem],
        );
    }
}

/// Thin wrapper that exposes the parts of [`Session`] task runners need.
#[derive(Clone)]
pub(crate) struct AgentTaskContext {
    session: Arc<Session>,
}

impl AgentTaskContext {
    pub(crate) fn new(session: Arc<Session>) -> Self {
        Self { session }
    }

    pub(crate) fn clone_session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }

    pub(crate) fn auth_manager(&self) -> Arc<AuthManager> {
        Arc::clone(&self.session.services.auth_manager)
    }

    pub(crate) fn models_manager(&self) -> Arc<ModelsManager> {
        Arc::clone(&self.session.services.models_manager)
    }
}

/// Async task that drives a [`Session`] turn.
///
/// Implementations encapsulate a specific Praxis workflow (regular chat,
/// reviews, ghost snapshots, etc.). Each task instance is owned by a
/// [`Session`] and executed on a background Tokio task. The trait is
/// intentionally small: implementers identify themselves via
/// [`AgentTask::kind`], perform their work in [`AgentTask::run`], and may
/// release resources in [`AgentTask::abort`].
#[async_trait]
pub(crate) trait AgentTask: Send + Sync + 'static {
    /// Describes the type of work the task performs so the session can
    /// surface it in telemetry and UI.
    fn kind(&self) -> AgentTaskKind;

    /// Returns the tracing name for a spawned task span.
    fn span_name(&self) -> &'static str;

    /// Executes the task until completion or cancellation.
    ///
    /// Implementations typically stream protocol events using `session` and
    /// `ctx`, returning an optional final agent message when finished. The
    /// provided `cancellation_token` is cancelled when the session requests an
    /// abort; implementers should watch for it and terminate quickly once it
    /// fires. Returning [`Some`] yields a final message that
    /// [`Session::on_task_finished`] will emit to the client.
    async fn run(
        self: Arc<Self>,
        session: Arc<AgentTaskContext>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;

    /// Gives the task a chance to perform cleanup after an abort.
    ///
    /// The default implementation is a no-op; override this if additional
    /// teardown or notifications are required once
    /// [`Session::abort_all_tasks`] cancels the task.
    async fn abort(&self, session: Arc<AgentTaskContext>, ctx: Arc<TurnContext>) {
        let _ = (session, ctx);
    }
}

struct FinishedTaskState {
    pending_input: Vec<ResponseInputItem>,
    should_schedule_pending_work: bool,
    token_usage_at_turn_start: Option<TokenUsage>,
    turn_tool_calls: u64,
}

impl Session {
    pub async fn spawn_task<T: AgentTask>(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
        task: T,
    ) {
        self.abort_all_tasks(TurnAbortReason::Replaced).await;
        self.clear_connector_selection().await;
        self.start_task(turn_context, input, task).await;
    }

    async fn start_task<T: AgentTask>(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        input: Vec<UserInput>,
        task: T,
    ) {
        let task: Arc<dyn AgentTask> = Arc::new(task);
        let task_kind = task.kind();
        let span_name = task.span_name();
        let started_at = Instant::now();
        turn_context
            .turn_timing_state
            .mark_turn_started(started_at)
            .await;
        let token_usage_at_turn_start = self.total_token_usage().await.unwrap_or_default();
        if let Err(err) = self
            .goal_runtime_apply(GoalRuntimeEvent::TurnStarted {
                turn_context: turn_context.as_ref(),
                token_usage: token_usage_at_turn_start.clone(),
            })
            .await
        {
            warn!("failed to apply goal turn-start runtime event: {err}");
        }

        let cancellation_token = CancellationToken::new();
        let done = Arc::new(Notify::new());

        let queued_response_items = self.take_queued_response_items_for_next_turn().await;
        let mailbox_items = self.get_pending_input().await;
        let turn_state = {
            let mut active = self.active_turn.lock().await;
            let turn = active.get_or_insert_with(ActiveTurn::default);
            debug_assert!(turn.tasks.is_empty());
            Arc::clone(&turn.turn_state)
        };
        {
            let mut turn_state = turn_state.lock().await;
            turn_state.token_usage_at_turn_start = token_usage_at_turn_start;
            for item in queued_response_items {
                turn_state.push_pending_input(item);
            }
            for item in mailbox_items {
                turn_state.push_pending_input(item);
            }
        }

        let mut active = self.active_turn.lock().await;
        let turn = active.get_or_insert_with(ActiveTurn::default);
        debug_assert!(turn.tasks.is_empty());
        let done_clone = Arc::clone(&done);
        let session_ctx = Arc::new(AgentTaskContext::new(Arc::clone(self)));
        let ctx = Arc::clone(&turn_context);
        let task_for_run = Arc::clone(&task);
        let task_cancellation_token = cancellation_token.child_token();
        // Task-owned turn spans keep a core-owned span open for the
        // full task lifecycle after the submission dispatch span ends.
        let task_span = info_span!(
            "turn",
            otel.name = span_name,
            thread.id = %self.conversation_id,
            turn.id = %turn_context.sub_id,
            model = %turn_context.model_info.slug,
        );
        let handle = tokio::spawn(
            async move {
                let ctx_for_finish = Arc::clone(&ctx);
                let last_agent_message = task_for_run
                    .run(
                        Arc::clone(&session_ctx),
                        ctx,
                        input,
                        task_cancellation_token.child_token(),
                    )
                    .await;
                let sess = session_ctx.clone_session();
                sess.flush_rollout().await;
                if !task_cancellation_token.is_cancelled() {
                    // Emit completion uniformly from spawn site so all tasks share the same lifecycle.
                    sess.on_task_finished(Arc::clone(&ctx_for_finish), last_agent_message)
                        .await;
                }
                done_clone.notify_waiters();
            }
            .instrument(task_span),
        );
        let timer = turn_context
            .session_telemetry
            .start_timer(TURN_E2E_DURATION_METRIC, &[])
            .ok();
        let running_task = RunningAgentTask::new(
            done,
            task_kind,
            task,
            cancellation_token,
            handle.abort_handle(),
            Arc::clone(&turn_context),
            timer,
        );
        turn.add_task(running_task);
    }

    /// Starts a regular turn when the session is idle and pending work is waiting.
    ///
    /// Pending work includes queued next-turn items, runtime commands, and mailbox mail marked
    /// with `trigger_turn`.
    ///
    /// This helper generates a fresh sub-id for the synthetic turn before delegating to the
    /// explicit-sub-id variant.
    pub(crate) async fn maybe_start_turn_for_pending_work(self: &Arc<Self>) {
        self.maybe_start_turn_for_pending_work_with_sub_id(uuid::Uuid::new_v4().to_string())
            .await;
    }

    /// Starts a regular turn with the provided sub-id when pending work should wake an idle
    /// session.
    ///
    /// The turn is created only when structured pending work should wake the session, and only if
    /// the session is currently idle.
    pub(crate) async fn maybe_start_turn_for_pending_work_with_sub_id(
        self: &Arc<Self>,
        sub_id: String,
    ) {
        let has_queued_response_items = self.has_queued_response_items_for_next_turn().await;
        let has_runtime_command = self
            .services
            .agent_os
            .has_claimable_runtime_command_for_thread(self.conversation_id)
            .await;
        let has_trigger_turn_mailbox_items = self.has_trigger_turn_mailbox_items().await;
        if !has_queued_response_items && !has_runtime_command && !has_trigger_turn_mailbox_items {
            return;
        }

        {
            let mut active_turn = self.active_turn.lock().await;
            if active_turn.is_some() {
                return;
            }
            *active_turn = Some(ActiveTurn::default());
        }

        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;
        self.maybe_emit_unknown_model_warning_for_turn(turn_context.as_ref())
            .await;
        self.start_task(turn_context, Vec::new(), RegularAgentTask::new())
            .await;
    }

    pub async fn abort_all_tasks(self: &Arc<Self>, reason: TurnAbortReason) {
        if let Some(mut active_turn) = self.take_active_turn().await {
            for task in active_turn.drain_tasks() {
                self.handle_task_abort(task, reason.clone()).await;
            }
            // Let interrupted tasks observe cancellation before dropping pending approvals, or an
            // in-flight approval wait can surface as a model-visible rejection before TurnAborted.
            active_turn.clear_pending().await;
        }
        if reason == TurnAbortReason::Interrupted {
            self.maybe_start_turn_for_pending_work().await;
        }
    }

    pub async fn on_task_finished(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        mut last_agent_message: Option<String>,
    ) {
        turn_context
            .turn_metadata_state
            .cancel_git_enrichment_task();

        let finished_task_state = self.take_finished_task_state(&turn_context).await;
        self.record_finished_task_pending_input(&turn_context, finished_task_state.pending_input)
            .await;
        let terminal_model_error = turn_context.tool_loop_guard.terminal_model_error_message();
        let turn_token_usage = self
            .emit_finished_turn_metrics(
                finished_task_state.token_usage_at_turn_start,
                finished_task_state.turn_tool_calls,
            )
            .await;
        if last_agent_message.is_none() && terminal_model_error.is_none() {
            if let Some(final_item) =
                synthetic_final_item_for_guard(Arc::clone(self), &turn_context, true).await
            {
                last_agent_message =
                    emit_synthetic_final_answer(self, &turn_context, final_item).await;
            }
        }
        let last_agent_message_for_title = last_agent_message.clone();
        let last_agent_message_for_summary = last_agent_message.clone();
        let turn_completed = terminal_model_error.is_none();
        let event = EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: turn_context.sub_id.clone(),
            last_agent_message,
        });
        if let Err(err) = self
            .goal_runtime_apply(GoalRuntimeEvent::TurnFinished {
                turn_context: turn_context.as_ref(),
                turn_completed,
            })
            .await
        {
            warn!("failed to apply goal turn-finished runtime event: {err}");
        }
        self.send_event(turn_context.as_ref(), event).await;
        if let Err(err) = self
            .services
            .agent_os
            .complete_active_runtime_command_for_thread(
                self.conversation_id,
                turn_completed,
                if turn_completed {
                    "turn_finished"
                } else {
                    "turn_model_error"
                },
            )
            .await
        {
            warn!("failed to complete AgentOS runtime command for finished turn: {err}");
        }

        crate::thread_cost::persist_turn_cost_estimate(
            self,
            &turn_context.model_info.slug,
            turn_token_usage.as_ref(),
        )
        .await;
        crate::auto_title::maybe_auto_generate_title(self, last_agent_message_for_title).await;
        crate::auto_summary::maybe_auto_generate_summary(self, last_agent_message_for_summary)
            .await;

        if finished_task_state.should_schedule_pending_work {
            self.schedule_pending_work_continuation();
        }
    }

    async fn take_finished_task_state(&self, turn_context: &TurnContext) -> FinishedTaskState {
        let turn_state = {
            let mut active = self.active_turn.lock().await;
            if let Some(at) = active.as_mut()
                && at.remove_task(&turn_context.sub_id)
            {
                let turn_state = Arc::clone(&at.turn_state);
                *active = None;
                Some(turn_state)
            } else {
                None
            }
        };
        let Some(turn_state) = turn_state else {
            return FinishedTaskState {
                pending_input: Vec::new(),
                should_schedule_pending_work: false,
                token_usage_at_turn_start: None,
                turn_tool_calls: 0,
            };
        };
        let mut turn_state = turn_state.lock().await;
        FinishedTaskState {
            pending_input: turn_state.take_pending_input(),
            should_schedule_pending_work: true,
            token_usage_at_turn_start: Some(turn_state.token_usage_at_turn_start.clone()),
            turn_tool_calls: turn_state.tool_calls,
        }
    }

    async fn record_finished_task_pending_input(
        self: &Arc<Self>,
        turn_context: &Arc<TurnContext>,
        pending_input: Vec<ResponseInputItem>,
    ) {
        record_pending_inputs(self, turn_context, pending_input).await;
    }

    async fn emit_finished_turn_metrics(
        &self,
        token_usage_at_turn_start: Option<TokenUsage>,
        turn_tool_calls: u64,
    ) -> Option<TokenUsage> {
        let token_usage_at_turn_start = token_usage_at_turn_start?;
        // TODO(jif): drop this
        let tmp_mem = (
            "tmp_mem_enabled",
            if self.enabled(Feature::MemoryTool) {
                "true"
            } else {
                "false"
            },
        );
        let network_proxy_active = match self.services.network_proxy.as_ref() {
            Some(started_network_proxy) => {
                match started_network_proxy.proxy().current_cfg().await {
                    Ok(config) => config.network.enabled,
                    Err(err) => {
                        warn!(
                            "failed to read managed network proxy state for turn metrics: {err:#}"
                        );
                        false
                    }
                }
            }
            None => false,
        };
        emit_turn_network_proxy_metric(
            &self.services.session_telemetry,
            network_proxy_active,
            tmp_mem,
        );
        self.services.session_telemetry.histogram(
            TURN_TOOL_CALL_METRIC,
            i64::try_from(turn_tool_calls).unwrap_or(i64::MAX),
            &[tmp_mem],
        );
        let total_token_usage = self.total_token_usage().await.unwrap_or_default();
        let computed_turn_token_usage = TokenUsage {
            input_tokens: (total_token_usage.input_tokens - token_usage_at_turn_start.input_tokens)
                .max(0),
            cached_input_tokens: (total_token_usage.cached_input_tokens
                - token_usage_at_turn_start.cached_input_tokens)
                .max(0),
            cache_reported_input_tokens: (total_token_usage.cache_reported_input_tokens
                - token_usage_at_turn_start.cache_reported_input_tokens)
                .max(0),
            output_tokens: (total_token_usage.output_tokens
                - token_usage_at_turn_start.output_tokens)
                .max(0),
            reasoning_output_tokens: (total_token_usage.reasoning_output_tokens
                - token_usage_at_turn_start.reasoning_output_tokens)
                .max(0),
            total_tokens: (total_token_usage.total_tokens - token_usage_at_turn_start.total_tokens)
                .max(0),
        };
        emit_turn_token_usage_metrics(
            &self.services.session_telemetry,
            &computed_turn_token_usage,
            tmp_mem,
        );
        Some(computed_turn_token_usage)
    }

    fn schedule_pending_work_continuation(self: &Arc<Self>) {
        let session = Arc::clone(self);
        let _scheduler = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                session.maybe_start_turn_for_pending_work().await;
                if let Err(err) = session
                    .goal_runtime_apply(GoalRuntimeEvent::MaybeContinueIfIdle)
                    .await
                {
                    warn!("failed to apply goal idle-continuation runtime event: {err}");
                }
            });
        });
    }

    async fn take_active_turn(&self) -> Option<ActiveTurn> {
        let mut active = self.active_turn.lock().await;
        active.take()
    }

    pub(crate) async fn close_unified_exec_processes(&self) {
        self.services
            .unified_exec_manager
            .terminate_all_processes()
            .await;
    }

    pub(crate) async fn cleanup_after_interrupt(&self, turn_context: &Arc<TurnContext>) {
        let _ = turn_context;
    }

    async fn handle_task_abort(self: &Arc<Self>, task: RunningAgentTask, reason: TurnAbortReason) {
        let sub_id = task.turn_context.sub_id.clone();
        if task.cancellation_token.is_cancelled() {
            return;
        }

        trace!(task_kind = ?task.kind, sub_id, "aborting running task");
        task.cancellation_token.cancel();
        task.turn_context
            .turn_metadata_state
            .cancel_git_enrichment_task();
        let agent_task = Arc::clone(&task.task);

        select! {
            _ = task.done.notified() => {
            },
            _ = tokio::time::sleep(Duration::from_millis(GRACEFULL_INTERRUPTION_TIMEOUT_MS)) => {
                warn!("task {sub_id} didn't complete gracefully after {}ms", GRACEFULL_INTERRUPTION_TIMEOUT_MS);
            }
        }

        task.handle.abort();

        let session_ctx = Arc::new(AgentTaskContext::new(Arc::clone(self)));
        agent_task
            .abort(session_ctx, Arc::clone(&task.turn_context))
            .await;
        self.services
            .agent_os
            .cleanup_thread_resources_after_abort(
                self.conversation_id,
                format!("turn_aborted:{reason:?}"),
            )
            .await;
        if let Err(err) = self
            .goal_runtime_apply(GoalRuntimeEvent::TaskAborted {
                turn_context: Some(task.turn_context.as_ref()),
            })
            .await
        {
            warn!("failed to apply goal task-aborted runtime event: {err}");
        }

        if reason == TurnAbortReason::Interrupted {
            self.cleanup_after_interrupt(&task.turn_context).await;

            let marker = interrupted_turn_history_marker();
            self.record_into_history(std::slice::from_ref(&marker), task.turn_context.as_ref())
                .await;
            self.persist_rollout_items(&[RolloutItem::ResponseItem(marker)])
                .await;
            // Ensure the marker is durably visible before emitting TurnAborted: some clients
            // synchronously re-read the rollout on receipt of the abort event.
            self.flush_rollout().await;
        }

        let event = EventMsg::TurnAborted(TurnAbortedEvent {
            turn_id: Some(task.turn_context.sub_id.clone()),
            reason,
        });
        self.send_event(task.turn_context.as_ref(), event).await;
        if let Err(err) = self
            .services
            .agent_os
            .complete_active_runtime_command_for_thread(
                self.conversation_id,
                /*succeeded*/ false,
                "turn_aborted",
            )
            .await
        {
            warn!("failed to fail AgentOS runtime command for aborted turn: {err}");
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
