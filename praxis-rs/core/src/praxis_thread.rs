use crate::agent::AgentStatus;
use crate::config::ConstraintResult;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::file_watcher::WatchRegistration;
use crate::praxis::Praxis;
use crate::praxis::SteerInputError;
use praxis_features::Feature;
use praxis_protocol::config_types::ApprovalsReviewer;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::Submission;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::W3cTraceContext;
use praxis_protocol::user_input::UserInput;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::sync::watch;

use praxis_rollout::state_db::StateDbHandle;

#[derive(Clone, Debug)]
pub struct ThreadConfigSnapshot {
    pub model: String,
    pub model_provider_id: String,
    pub service_tier: Option<ServiceTier>,
    pub approval_policy: AskForApproval,
    pub approvals_reviewer: ApprovalsReviewer,
    pub sandbox_policy: SandboxPolicy,
    pub cwd: PathBuf,
    pub ephemeral: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub personality: Option<Personality>,
    pub session_source: SessionSource,
}

pub struct PraxisThread {
    pub(crate) praxis: Praxis,
    rollout_path: Option<PathBuf>,
    out_of_band_elicitation_count: Mutex<u64>,
    _watch_registration: WatchRegistration,
}

/// Conduit for the bidirectional stream of messages that compose a thread
/// (formerly called a conversation) in Praxis.
impl PraxisThread {
    pub(crate) fn new(
        praxis: Praxis,
        rollout_path: Option<PathBuf>,
        watch_registration: WatchRegistration,
    ) -> Self {
        Self {
            praxis,
            rollout_path,
            out_of_band_elicitation_count: Mutex::new(0),
            _watch_registration: watch_registration,
        }
    }

    pub async fn submit(&self, op: Op) -> PraxisResult<String> {
        self.praxis.submit(op).await
    }

    pub async fn shutdown_and_wait(&self) -> PraxisResult<()> {
        self.praxis.shutdown_and_wait().await
    }

    #[doc(hidden)]
    pub async fn ensure_rollout_materialized(&self) {
        self.praxis.session.ensure_rollout_materialized().await;
    }

    #[doc(hidden)]
    pub async fn flush_rollout(&self) {
        self.praxis.session.flush_rollout().await;
    }

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> PraxisResult<String> {
        self.praxis.submit_with_trace(op, trace).await
    }

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
    ) -> Result<String, SteerInputError> {
        self.praxis.steer_input(input, expected_turn_id).await
    }

    pub async fn set_app_gateway_client_name(
        &self,
        app_gateway_client_name: Option<String>,
    ) -> ConstraintResult<()> {
        self.praxis
            .set_app_gateway_client_name(app_gateway_client_name)
            .await
    }

    /// Use sparingly: this is intended to be removed soon.
    pub async fn submit_with_id(&self, sub: Submission) -> PraxisResult<()> {
        self.praxis.submit_with_id(sub).await
    }

    pub async fn next_event(&self) -> PraxisResult<Event> {
        self.praxis.next_event().await
    }

    pub async fn agent_status(&self) -> AgentStatus {
        self.praxis.agent_status().await
    }

    pub(crate) fn subscribe_status(&self) -> watch::Receiver<AgentStatus> {
        self.praxis.agent_status.clone()
    }

    pub(crate) async fn total_token_usage(&self) -> Option<TokenUsage> {
        self.praxis.session.total_token_usage().await
    }

    /// Records a user-role session-prefix message without creating a new user turn boundary.
    pub(crate) async fn inject_user_message_without_turn(&self, message: String) {
        let message = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: message }],
            end_turn: None,
            phase: None,
        };
        let pending_item = match pending_message_input_item(&message) {
            Ok(pending_item) => pending_item,
            Err(err) => {
                debug_assert!(false, "session-prefix message append should succeed: {err}");
                return;
            }
        };
        if self
            .praxis
            .session
            .inject_response_items(vec![pending_item])
            .await
            .is_err()
        {
            let turn_context = self.praxis.session.new_default_turn().await;
            self.praxis
                .session
                .record_conversation_items(turn_context.as_ref(), &[message])
                .await;
        }
    }

    /// Append a prebuilt message to the thread history without treating it as a user turn.
    ///
    /// If the thread already has an active turn, the message is queued as pending input for that
    /// turn. Otherwise it is queued at session scope and a regular turn is started so the agent
    /// can consume that pending input through the normal turn pipeline.
    #[cfg(test)]
    pub(crate) async fn append_message(&self, message: ResponseItem) -> PraxisResult<String> {
        let submission_id = uuid::Uuid::new_v4().to_string();
        let pending_item = pending_message_input_item(&message)?;
        if let Err(items) = self
            .praxis
            .session
            .inject_response_items(vec![pending_item])
            .await
        {
            self.praxis
                .session
                .queue_response_items_for_next_turn(items)
                .await;
            self.praxis
                .session
                .maybe_start_turn_for_pending_work()
                .await;
        }

        Ok(submission_id)
    }

    pub fn rollout_path(&self) -> Option<PathBuf> {
        self.rollout_path.clone()
    }

    pub fn state_db(&self) -> Option<StateDbHandle> {
        self.praxis.state_db()
    }

    pub async fn regenerate_thread_name(&self) -> anyhow::Result<String> {
        crate::auto_title::regenerate_thread_title(&self.praxis.session).await
    }

    pub async fn thread_title_preview(&self) -> Option<String> {
        let history = self.praxis.session.clone_history().await;
        crate::auto_title::title_preview_from_response_items(history.raw_items())
    }

    pub async fn config_snapshot(&self) -> ThreadConfigSnapshot {
        self.praxis.thread_config_snapshot().await
    }

    pub fn enabled(&self, feature: Feature) -> bool {
        self.praxis.enabled(feature)
    }

    pub async fn increment_out_of_band_elicitation_count(&self) -> PraxisResult<u64> {
        let mut guard = self.out_of_band_elicitation_count.lock().await;
        let was_zero = *guard == 0;
        *guard = guard.checked_add(1).ok_or_else(|| {
            PraxisErr::Fatal("out-of-band elicitation count overflowed".to_string())
        })?;

        if was_zero {
            self.praxis
                .session
                .set_out_of_band_elicitation_pause_state(/*paused*/ true);
        }

        Ok(*guard)
    }

    pub async fn decrement_out_of_band_elicitation_count(&self) -> PraxisResult<u64> {
        let mut guard = self.out_of_band_elicitation_count.lock().await;
        if *guard == 0 {
            return Err(PraxisErr::InvalidRequest(
                "out-of-band elicitation count is already zero".to_string(),
            ));
        }

        *guard -= 1;
        let now_zero = *guard == 0;
        if now_zero {
            self.praxis
                .session
                .set_out_of_band_elicitation_pause_state(/*paused*/ false);
        }

        Ok(*guard)
    }
}

fn pending_message_input_item(message: &ResponseItem) -> PraxisResult<ResponseInputItem> {
    match message {
        ResponseItem::Message { role, content, .. } => Ok(ResponseInputItem::Message {
            role: role.clone(),
            content: content.clone(),
        }),
        _ => Err(PraxisErr::InvalidRequest(
            "append_message only supports ResponseItem::Message".to_string(),
        )),
    }
}
