use crate::bottom_pane::FeedbackAudience;
use crate::status::StatusAccountDisplay;
use crate::status::plan_type_display_name;
use color_eyre::eyre::ContextCompat;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use praxis_app_gateway_client::AppGatewayClient;
use praxis_app_gateway_client::AppGatewayEvent;
use praxis_app_gateway_client::AppGatewayRequestHandle;
use praxis_app_gateway_client::RemoteAppGatewayClient;
use praxis_app_gateway_client::RemoteAppGatewayConnectArgs;
use praxis_app_gateway_client::TypedRequestError;
use praxis_app_gateway_protocol::Account;
use praxis_app_gateway_protocol::AuthMode;
use praxis_app_gateway_protocol::ClientRequest;
use praxis_app_gateway_protocol::ConfigBatchWriteParams;
use praxis_app_gateway_protocol::ConfigWriteResponse;
use praxis_app_gateway_protocol::GetAccountParams;
use praxis_app_gateway_protocol::GetAccountRateLimitsResponse;
use praxis_app_gateway_protocol::GetAccountResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::Model as ApiModel;
use praxis_app_gateway_protocol::ModelListParams;
use praxis_app_gateway_protocol::ModelListResponse;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ReviewDelivery;
use praxis_app_gateway_protocol::ReviewStartParams;
use praxis_app_gateway_protocol::ReviewStartResponse;
use praxis_app_gateway_protocol::SkillsListParams;
use praxis_app_gateway_protocol::SkillsListResponse;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadArchiveParams;
use praxis_app_gateway_protocol::ThreadArchiveResponse;
use praxis_app_gateway_protocol::ThreadBackgroundTerminalsCleanParams;
use praxis_app_gateway_protocol::ThreadBackgroundTerminalsCleanResponse;
use praxis_app_gateway_protocol::ThreadCompactStartParams;
use praxis_app_gateway_protocol::ThreadCompactStartResponse;
use praxis_app_gateway_protocol::ThreadControlReleaseParams;
use praxis_app_gateway_protocol::ThreadControlReleaseResponse;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadDeleteParams;
use praxis_app_gateway_protocol::ThreadDeleteResponse;
use praxis_app_gateway_protocol::ThreadForkParams;
use praxis_app_gateway_protocol::ThreadForkResponse;
use praxis_app_gateway_protocol::ThreadGoal;
use praxis_app_gateway_protocol::ThreadGoalClearParams;
use praxis_app_gateway_protocol::ThreadGoalClearResponse;
use praxis_app_gateway_protocol::ThreadGoalGetParams;
use praxis_app_gateway_protocol::ThreadGoalGetResponse;
use praxis_app_gateway_protocol::ThreadGoalSetParams;
use praxis_app_gateway_protocol::ThreadGoalSetResponse;
use praxis_app_gateway_protocol::ThreadGoalStatus;
use praxis_app_gateway_protocol::ThreadGoalUpdateParams;
use praxis_app_gateway_protocol::ThreadGoalUpdateResponse;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadLoadedListParams;
use praxis_app_gateway_protocol::ThreadLoadedListResponse;
use praxis_app_gateway_protocol::ThreadMetadataUpdateParams;
use praxis_app_gateway_protocol::ThreadMetadataUpdateResponse;
use praxis_app_gateway_protocol::ThreadReadParams;
use praxis_app_gateway_protocol::ThreadReadResponse;
use praxis_app_gateway_protocol::ThreadRealtimeAppendAudioParams;
use praxis_app_gateway_protocol::ThreadRealtimeAppendAudioResponse;
use praxis_app_gateway_protocol::ThreadRealtimeAppendTextParams;
use praxis_app_gateway_protocol::ThreadRealtimeAppendTextResponse;
use praxis_app_gateway_protocol::ThreadRealtimeStartParams;
use praxis_app_gateway_protocol::ThreadRealtimeStartResponse;
use praxis_app_gateway_protocol::ThreadRealtimeStopParams;
use praxis_app_gateway_protocol::ThreadRealtimeStopResponse;
use praxis_app_gateway_protocol::ThreadRegenerateNameParams;
use praxis_app_gateway_protocol::ThreadRegenerateNameResponse;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadResumeResponse;
use praxis_app_gateway_protocol::ThreadRollbackParams;
use praxis_app_gateway_protocol::ThreadRollbackResponse;
use praxis_app_gateway_protocol::ThreadSetNameParams;
use praxis_app_gateway_protocol::ThreadSetNameResponse;
use praxis_app_gateway_protocol::ThreadShellCommandParams;
use praxis_app_gateway_protocol::ThreadShellCommandResponse;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_app_gateway_protocol::ThreadUnsubscribeParams;
use praxis_app_gateway_protocol::ThreadUnsubscribeResponse;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnInterruptParams;
use praxis_app_gateway_protocol::TurnInterruptResponse;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::TurnSteerParams;
use praxis_app_gateway_protocol::TurnSteerResponse;
use praxis_core::config::Config;
use praxis_core::message_history;
use praxis_otel::TelemetryAuthMode;
use praxis_protocol::ThreadId;
use praxis_protocol::openai_models::ModelAvailabilityNux;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ModelUpgrade;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::ConversationAudioParams;
use praxis_protocol::protocol::ConversationStartParams;
use praxis_protocol::protocol::ConversationTextParams;
use praxis_protocol::protocol::CreditsSnapshot;
use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::RateLimitWindow;
use praxis_protocol::protocol::ReviewRequest;
use praxis_protocol::protocol::ReviewTarget as CoreReviewTarget;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionNetworkProxyRuntime;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct AppGatewayBootstrap {
    pub(crate) account_auth_mode: Option<AuthMode>,
    pub(crate) account_email: Option<String>,
    pub(crate) auth_mode: Option<TelemetryAuthMode>,
    pub(crate) status_account_display: Option<StatusAccountDisplay>,
    pub(crate) plan_type: Option<praxis_protocol::account::PlanType>,
    pub(crate) default_model: String,
    pub(crate) feedback_audience: FeedbackAudience,
    pub(crate) has_chatgpt_account: bool,
    pub(crate) available_models: Vec<ModelPreset>,
    pub(crate) rate_limit_snapshots: Vec<RateLimitSnapshot>,
}

pub(crate) struct AppGatewaySession {
    client: AppGatewayClient,
    next_request_id: i64,
}

pub(crate) fn token_usage_info_from_app_gateway(token_usage: ThreadTokenUsage) -> TokenUsageInfo {
    TokenUsageInfo {
        total_token_usage: token_usage_from_app_gateway(token_usage.total),
        last_token_usage: token_usage_from_app_gateway(token_usage.last),
        model_context_window: token_usage.model_context_window,
        model_auto_compact_token_limit: token_usage.model_auto_compact_token_limit,
    }
}

fn token_usage_from_app_gateway(
    value: praxis_app_gateway_protocol::TokenUsageBreakdown,
) -> TokenUsage {
    TokenUsage {
        total_tokens: value.total_tokens,
        input_tokens: value.input_tokens,
        cached_input_tokens: value.cached_input_tokens,
        cache_reported_input_tokens: value.cache_reported_input_tokens,
        output_tokens: value.output_tokens,
        reasoning_output_tokens: value.reasoning_output_tokens,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ThreadSessionState {
    pub(crate) thread_id: ThreadId,
    pub(crate) forked_from_id: Option<ThreadId>,
    pub(crate) thread_name: Option<String>,
    pub(crate) model: String,
    pub(crate) model_provider_id: String,
    pub(crate) service_tier: Option<praxis_protocol::config_types::ServiceTier>,
    pub(crate) approval_policy: AskForApproval,
    pub(crate) approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) cwd: PathBuf,
    pub(crate) reasoning_effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
    pub(crate) history_log_id: u64,
    pub(crate) history_entry_count: u64,
    pub(crate) network_proxy: Option<SessionNetworkProxyRuntime>,
    pub(crate) rollout_path: Option<PathBuf>,
    pub(crate) selfwork_plan_path: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum ThreadParamsMode {
    Embedded,
    Remote,
}

impl ThreadParamsMode {
    fn model_provider_from_config(self, config: &Config) -> Option<String> {
        match self {
            Self::Embedded => Some(config.model_provider_id.clone()),
            Self::Remote => None,
        }
    }
}

pub(crate) struct AppGatewayStartedThread {
    pub(crate) session: ThreadSessionState,
    pub(crate) turns: Vec<Turn>,
    pub(crate) status: ThreadStatus,
    pub(crate) control_state: Option<ThreadControlState>,
}

impl AppGatewaySession {
    pub(crate) fn new(client: AppGatewayClient) -> Self {
        Self {
            client,
            next_request_id: 1,
        }
    }

    pub(crate) fn is_remote(&self) -> bool {
        matches!(self.client, AppGatewayClient::Remote(_))
    }

    pub(crate) async fn reconnect_remote(
        &mut self,
        websocket_url: String,
        auth_token: Option<String>,
    ) -> Result<()> {
        let next_client = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
            websocket_url,
            auth_token,
            client_name: "praxis-tui".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            experimental_api: true,
            opt_out_notification_methods: Vec::new(),
            channel_capacity: crate::TUI_APP_GATEWAY_CHANNEL_CAPACITY,
        })
        .await
        .wrap_err("failed to reconnect to remote app gateway")?;
        let previous = std::mem::replace(&mut self.client, AppGatewayClient::Remote(next_client));
        self.next_request_id = 1;
        if let Err(err) = previous.shutdown().await {
            tracing::debug!("previous app-gateway client shutdown after reconnect failed: {err}");
        }
        Ok(())
    }

    pub(crate) async fn bootstrap(&mut self, config: &Config) -> Result<AppGatewayBootstrap> {
        let account_request_id = self.next_request_id();
        let account: GetAccountResponse = self
            .client
            .request_typed(ClientRequest::GetAccount {
                request_id: account_request_id,
                params: GetAccountParams {
                    refresh_token: false,
                },
            })
            .await
            .wrap_err("account/read failed during TUI bootstrap")?;
        let model_request_id = self.next_request_id();
        let models: ModelListResponse = self
            .client
            .request_typed(ClientRequest::ModelList {
                request_id: model_request_id,
                params: ModelListParams {
                    cursor: None,
                    limit: None,
                    include_hidden: Some(true),
                },
            })
            .await
            .wrap_err("model/list failed during TUI bootstrap")?;
        let available_models = models
            .data
            .into_iter()
            .map(model_preset_from_api_model)
            .collect::<Vec<_>>();
        let default_model = config
            .model
            .clone()
            .or_else(|| {
                available_models
                    .iter()
                    .find(|model| model.is_default)
                    .map(|model| model.model.clone())
            })
            .or_else(|| available_models.first().map(|model| model.model.clone()))
            .wrap_err("model/list returned no models for TUI bootstrap")?;

        let (
            account_auth_mode,
            account_email,
            auth_mode,
            status_account_display,
            plan_type,
            feedback_audience,
            has_chatgpt_account,
        ) = match account.account {
            Some(Account::ApiKey {}) => (
                Some(AuthMode::ApiKey),
                None,
                Some(TelemetryAuthMode::ApiKey),
                Some(StatusAccountDisplay::ApiKey),
                None,
                FeedbackAudience::External,
                false,
            ),
            Some(Account::Chatgpt { email, plan_type }) => {
                let feedback_audience = if email.ends_with("@openai.com") {
                    FeedbackAudience::OpenAiEmployee
                } else {
                    FeedbackAudience::External
                };
                (
                    Some(AuthMode::Chatgpt),
                    Some(email.clone()),
                    Some(TelemetryAuthMode::Chatgpt),
                    Some(StatusAccountDisplay::ChatGpt {
                        email: Some(email),
                        plan: Some(plan_type_display_name(plan_type)),
                    }),
                    Some(plan_type),
                    feedback_audience,
                    true,
                )
            }
            None => (
                None,
                None,
                None,
                None,
                None,
                FeedbackAudience::External,
                false,
            ),
        };
        let rate_limit_snapshots = if account.requires_openai_auth && has_chatgpt_account {
            let rate_limit_request_id = self.next_request_id();
            match self
                .client
                .request_typed(ClientRequest::GetAccountRateLimits {
                    request_id: rate_limit_request_id,
                    params: None,
                })
                .await
            {
                Ok(rate_limits) => app_gateway_rate_limit_snapshots_to_core(rate_limits),
                Err(err) => {
                    tracing::warn!("account/rateLimits/read failed during TUI bootstrap: {err}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(AppGatewayBootstrap {
            account_auth_mode,
            account_email,
            auth_mode,
            status_account_display,
            plan_type,
            default_model,
            feedback_audience,
            has_chatgpt_account,
            available_models,
            rate_limit_snapshots,
        })
    }

    pub(crate) async fn next_event(&mut self) -> Option<AppGatewayEvent> {
        self.client.next_event().await
    }

    pub(crate) fn try_next_event(&mut self) -> Option<AppGatewayEvent> {
        self.client.try_next_event()
    }

    pub(crate) async fn start_thread(
        &mut self,
        config: &Config,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params_from_config(config, self.thread_params_mode()),
            })
            .await
            .wrap_err("thread/start failed during TUI bootstrap")?;
        started_thread_from_start_response(response, config).await
    }

    pub(crate) async fn resume_thread(
        &mut self,
        config: Config,
        thread_id: ThreadId,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadResumeResponse = self
            .client
            .request_typed(ClientRequest::ThreadResume {
                request_id,
                params: thread_resume_params_from_config(
                    config.clone(),
                    thread_id,
                    self.thread_params_mode(),
                ),
            })
            .await
            .wrap_err("thread/resume failed during TUI bootstrap")?;
        started_thread_from_resume_response(response, &config).await
    }

    pub(crate) async fn watch_thread(
        &mut self,
        config: &Config,
        thread_id: ThreadId,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadResumeResponse = self
            .client
            .request_typed(ClientRequest::ThreadResume {
                request_id,
                params: ThreadResumeParams {
                    thread_id: thread_id.to_string(),
                    persist_extended_history: true,
                    ..ThreadResumeParams::default()
                },
            })
            .await
            .wrap_err("thread/resume watch failed in TUI")?;
        started_thread_from_resume_response(response, config).await
    }

    pub(crate) async fn fork_thread(
        &mut self,
        config: Config,
        thread_id: ThreadId,
        path: Option<PathBuf>,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadForkResponse = self
            .client
            .request_typed(ClientRequest::ThreadFork {
                request_id,
                params: thread_fork_params_from_config(
                    config.clone(),
                    thread_id,
                    path,
                    self.thread_params_mode(),
                ),
            })
            .await
            .wrap_err("thread/fork failed during TUI bootstrap")?;
        started_thread_from_fork_response(response, &config).await
    }

    fn thread_params_mode(&self) -> ThreadParamsMode {
        match &self.client {
            AppGatewayClient::Native(_) => ThreadParamsMode::Embedded,
            AppGatewayClient::Remote(_) => ThreadParamsMode::Remote,
        }
    }

    pub(crate) async fn thread_list(
        &mut self,
        params: ThreadListParams,
    ) -> Result<ThreadListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadList { request_id, params })
            .await
            .wrap_err("thread/list failed during TUI session lookup")
    }

    /// Lists thread ids that the app gateway currently holds in memory.
    ///
    /// Used by `App::backfill_loaded_subagent_threads` to discover subagent threads that were
    /// spawned before the TUI connected. The caller then fetches full metadata per thread via
    /// `thread_read` and walks the spawn tree.
    pub(crate) async fn thread_loaded_list(
        &mut self,
        params: ThreadLoadedListParams,
    ) -> Result<ThreadLoadedListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadLoadedList { request_id, params })
            .await
            .wrap_err("failed to list loaded threads from app gateway")
    }

    pub(crate) async fn thread_read(
        &mut self,
        thread_id: ThreadId,
        include_turns: bool,
    ) -> Result<Thread> {
        let request_id = self.next_request_id();
        let response: ThreadReadResponse = self
            .client
            .request_typed(ClientRequest::ThreadRead {
                request_id,
                params: ThreadReadParams {
                    thread_id: thread_id.to_string(),
                    include_turns,
                },
            })
            .await
            .wrap_err("thread/read failed during TUI session lookup")?;
        Ok(response.thread)
    }

    pub(crate) async fn thread_goal_get(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadGoal>> {
        let request_id = self.next_request_id();
        let response: ThreadGoalGetResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalGet {
                request_id,
                params: ThreadGoalGetParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/goal/get failed in TUI")?;
        Ok(response.goal)
    }

    pub(crate) async fn thread_goal_set(
        &mut self,
        thread_id: ThreadId,
        objective: String,
    ) -> Result<ThreadGoal> {
        let request_id = self.next_request_id();
        let response: ThreadGoalSetResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalSet {
                request_id,
                params: ThreadGoalSetParams {
                    thread_id: thread_id.to_string(),
                    objective,
                    token_budget: None,
                    clear_token_budget: false,
                },
            })
            .await
            .wrap_err("thread/goal/set failed in TUI")?;
        Ok(response.goal)
    }

    pub(crate) async fn thread_goal_update(
        &mut self,
        thread_id: ThreadId,
        objective: Option<String>,
        status: Option<ThreadGoalStatus>,
        token_budget: Option<i64>,
        clear_token_budget: bool,
    ) -> Result<ThreadGoal> {
        let request_id = self.next_request_id();
        let response: ThreadGoalUpdateResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalUpdate {
                request_id,
                params: ThreadGoalUpdateParams {
                    thread_id: thread_id.to_string(),
                    objective,
                    status,
                    token_budget,
                    clear_token_budget,
                },
            })
            .await
            .wrap_err("thread/goal/update failed in TUI")?;
        Ok(response.goal)
    }

    pub(crate) async fn thread_goal_clear(&mut self, thread_id: ThreadId) -> Result<bool> {
        let request_id = self.next_request_id();
        let response: ThreadGoalClearResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalClear {
                request_id,
                params: ThreadGoalClearParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/goal/clear failed in TUI")?;
        Ok(response.cleared)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn turn_start(
        &mut self,
        thread_id: ThreadId,
        items: Vec<praxis_protocol::user_input::UserInput>,
        cwd: PathBuf,
        approval_policy: AskForApproval,
        approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
        sandbox_policy: SandboxPolicy,
        model_provider: Option<String>,
        model: String,
        effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
        summary: Option<praxis_protocol::config_types::ReasoningSummary>,
        service_tier: Option<Option<praxis_protocol::config_types::ServiceTier>>,
        collaboration_mode: Option<praxis_protocol::config_types::CollaborationMode>,
        personality: Option<praxis_protocol::config_types::Personality>,
        output_schema: Option<serde_json::Value>,
    ) -> Result<TurnStartResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::TurnStart {
                request_id,
                params: TurnStartParams {
                    thread_id: thread_id.to_string(),
                    input: items.into_iter().map(Into::into).collect(),
                    cwd: Some(cwd),
                    approval_policy: Some(approval_policy.into()),
                    approvals_reviewer: Some(approvals_reviewer.into()),
                    sandbox_policy: Some(sandbox_policy.into()),
                    model_provider,
                    model: Some(model),
                    service_tier,
                    effort,
                    summary,
                    personality,
                    output_schema,
                    collaboration_mode,
                },
            })
            .await
            .wrap_err("turn/start failed in TUI")
    }

    pub(crate) async fn turn_interrupt(
        &mut self,
        thread_id: ThreadId,
        turn_id: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: TurnInterruptResponse = self
            .client
            .request_typed(ClientRequest::TurnInterrupt {
                request_id,
                params: TurnInterruptParams {
                    thread_id: thread_id.to_string(),
                    turn_id,
                },
            })
            .await
            .wrap_err("turn/interrupt failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn turn_steer(
        &mut self,
        thread_id: ThreadId,
        turn_id: String,
        items: Vec<praxis_protocol::user_input::UserInput>,
    ) -> std::result::Result<TurnSteerResponse, TypedRequestError> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::TurnSteer {
                request_id,
                params: TurnSteerParams {
                    thread_id: thread_id.to_string(),
                    input: items.into_iter().map(Into::into).collect(),
                    expected_turn_id: turn_id,
                },
            })
            .await
    }

    pub(crate) async fn thread_set_name(
        &mut self,
        thread_id: ThreadId,
        name: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadSetNameResponse = self
            .client
            .request_typed(ClientRequest::ThreadSetName {
                request_id,
                params: ThreadSetNameParams {
                    thread_id: thread_id.to_string(),
                    name,
                },
            })
            .await
            .wrap_err("thread/name/set failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_regenerate_name(&mut self, thread_id: ThreadId) -> Result<String> {
        let request_id = self.next_request_id();
        let response: ThreadRegenerateNameResponse = self
            .client
            .request_typed(ClientRequest::ThreadRegenerateName {
                request_id,
                params: ThreadRegenerateNameParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/name/regenerate failed in TUI")?;
        Ok(response.thread_name)
    }

    pub(crate) async fn thread_archive(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadArchiveResponse = self
            .client
            .request_typed(ClientRequest::ThreadArchive {
                request_id,
                params: ThreadArchiveParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/archive failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_delete(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadDeleteResponse = self
            .client
            .request_typed(ClientRequest::ThreadDelete {
                request_id,
                params: ThreadDeleteParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/delete failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_control_release(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadControlState>> {
        let request_id = self.next_request_id();
        let response: ThreadControlReleaseResponse = self
            .client
            .request_typed(ClientRequest::ThreadControlRelease {
                request_id,
                params: ThreadControlReleaseParams {
                    thread_id: thread_id.to_string(),
                    controller: None,
                },
            })
            .await
            .wrap_err("thread/control/release failed in TUI")?;
        Ok(response.previous_control_state)
    }

    pub(crate) async fn thread_set_selfwork_plan_path(
        &mut self,
        thread_id: ThreadId,
        selfwork_plan_path: Option<PathBuf>,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadMetadataUpdateResponse = self
            .client
            .request_typed(ClientRequest::ThreadMetadataUpdate {
                request_id,
                params: ThreadMetadataUpdateParams {
                    thread_id: thread_id.to_string(),
                    git_info: None,
                    selfwork_plan_path: Some(selfwork_plan_path),
                },
            })
            .await
            .wrap_err("thread/metadata/update failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_unsubscribe(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadUnsubscribeResponse = self
            .client
            .request_typed(ClientRequest::ThreadUnsubscribe {
                request_id,
                params: ThreadUnsubscribeParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/unsubscribe failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_compact_start(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadCompactStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadCompactStart {
                request_id,
                params: ThreadCompactStartParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/compact/start failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_shell_command(
        &mut self,
        thread_id: ThreadId,
        command: String,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadShellCommandResponse = self
            .client
            .request_typed(ClientRequest::ThreadShellCommand {
                request_id,
                params: ThreadShellCommandParams {
                    thread_id: thread_id.to_string(),
                    command,
                },
            })
            .await
            .wrap_err("thread/shellCommand failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_background_terminals_clean(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadBackgroundTerminalsCleanResponse = self
            .client
            .request_typed(ClientRequest::ThreadBackgroundTerminalsClean {
                request_id,
                params: ThreadBackgroundTerminalsCleanParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/backgroundTerminals/clean failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_rollback(
        &mut self,
        thread_id: ThreadId,
        num_turns: u32,
    ) -> Result<ThreadRollbackResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadRollback {
                request_id,
                params: ThreadRollbackParams {
                    thread_id: thread_id.to_string(),
                    num_turns,
                },
            })
            .await
            .wrap_err("thread/rollback failed in TUI")
    }

    pub(crate) async fn review_start(
        &mut self,
        thread_id: ThreadId,
        review_request: ReviewRequest,
    ) -> Result<ReviewStartResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ReviewStart {
                request_id,
                params: ReviewStartParams {
                    thread_id: thread_id.to_string(),
                    target: review_target_to_app_gateway(review_request.target),
                    delivery: Some(ReviewDelivery::Inline),
                },
            })
            .await
            .wrap_err("review/start failed in TUI")
    }

    pub(crate) async fn skills_list(
        &mut self,
        params: SkillsListParams,
    ) -> Result<SkillsListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::SkillsList { request_id, params })
            .await
            .wrap_err("skills/list failed in TUI")
    }

    pub(crate) async fn reload_user_config(&mut self) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ConfigWriteResponse = self
            .client
            .request_typed(ClientRequest::ConfigBatchWrite {
                request_id,
                params: ConfigBatchWriteParams {
                    edits: Vec::new(),
                    file_path: None,
                    expected_version: None,
                    reload_user_config: true,
                },
            })
            .await
            .wrap_err("config/batchWrite failed while reloading user config in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_start(
        &mut self,
        thread_id: ThreadId,
        params: ConversationStartParams,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeStart {
                request_id,
                params: ThreadRealtimeStartParams {
                    thread_id: thread_id.to_string(),
                    prompt: params.prompt,
                    session_id: params.session_id,
                },
            })
            .await
            .wrap_err("thread/realtime/start failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_audio(
        &mut self,
        thread_id: ThreadId,
        params: ConversationAudioParams,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeAppendAudioResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeAppendAudio {
                request_id,
                params: ThreadRealtimeAppendAudioParams {
                    thread_id: thread_id.to_string(),
                    audio: params.frame.into(),
                },
            })
            .await
            .wrap_err("thread/realtime/appendAudio failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_text(
        &mut self,
        thread_id: ThreadId,
        params: ConversationTextParams,
    ) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeAppendTextResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeAppendText {
                request_id,
                params: ThreadRealtimeAppendTextParams {
                    thread_id: thread_id.to_string(),
                    text: params.text,
                },
            })
            .await
            .wrap_err("thread/realtime/appendText failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn thread_realtime_stop(&mut self, thread_id: ThreadId) -> Result<()> {
        let request_id = self.next_request_id();
        let _: ThreadRealtimeStopResponse = self
            .client
            .request_typed(ClientRequest::ThreadRealtimeStop {
                request_id,
                params: ThreadRealtimeStopParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/realtime/stop failed in TUI")?;
        Ok(())
    }

    pub(crate) async fn reject_server_request(
        &self,
        request_id: RequestId,
        error: JSONRPCErrorError,
    ) -> std::io::Result<()> {
        self.client.reject_server_request(request_id, error).await
    }

    pub(crate) async fn resolve_server_request(
        &self,
        request_id: RequestId,
        result: serde_json::Value,
    ) -> std::io::Result<()> {
        self.client.resolve_server_request(request_id, result).await
    }

    pub(crate) async fn shutdown(self) -> std::io::Result<()> {
        self.client.shutdown().await
    }

    pub(crate) fn request_handle(&self) -> AppGatewayRequestHandle {
        self.client.request_handle()
    }

    fn next_request_id(&mut self) -> RequestId {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        RequestId::Integer(request_id)
    }
}

pub(crate) fn status_account_display_from_auth_mode(
    auth_mode: Option<AuthMode>,
    plan_type: Option<praxis_protocol::account::PlanType>,
) -> Option<StatusAccountDisplay> {
    match auth_mode {
        Some(AuthMode::ApiKey) => Some(StatusAccountDisplay::ApiKey),
        Some(AuthMode::Chatgpt) | Some(AuthMode::ChatgptAuthTokens) => {
            Some(StatusAccountDisplay::ChatGpt {
                email: None,
                plan: plan_type.map(plan_type_display_name),
            })
        }
        None => None,
    }
}

#[allow(dead_code)]
pub(crate) fn feedback_audience_from_account_email(
    account_email: Option<&str>,
) -> FeedbackAudience {
    match account_email {
        Some(email) if email.ends_with("@openai.com") => FeedbackAudience::OpenAiEmployee,
        Some(_) | None => FeedbackAudience::External,
    }
}

fn model_preset_from_api_model(model: ApiModel) -> ModelPreset {
    let upgrade = model.upgrade.map(|upgrade_id| {
        let upgrade_info = model.upgrade_info.clone();
        ModelUpgrade {
            id: upgrade_id,
            reasoning_effort_mapping: None,
            migration_config_key: model.model.clone(),
            model_link: upgrade_info
                .as_ref()
                .and_then(|info| info.model_link.clone()),
            upgrade_copy: upgrade_info
                .as_ref()
                .and_then(|info| info.upgrade_copy.clone()),
            migration_markdown: upgrade_info.and_then(|info| info.migration_markdown),
        }
    });

    ModelPreset {
        id: model.id,
        model: model.model,
        display_name: model.display_name,
        description: model.description,
        default_reasoning_effort: model.default_reasoning_effort,
        supported_reasoning_efforts: model
            .supported_reasoning_efforts
            .into_iter()
            .map(|effort| ReasoningEffortPreset {
                effort: effort.reasoning_effort,
                description: effort.description,
            })
            .collect(),
        supports_personality: model.supports_personality,
        is_default: model.is_default,
        upgrade,
        show_in_picker: !model.hidden,
        availability_nux: model.availability_nux.map(|nux| ModelAvailabilityNux {
            message: nux.message,
        }),
        // `model/list` already returns models filtered for the active client/auth context.
        supported_in_api: true,
        input_modalities: model.input_modalities,
    }
}

fn approvals_reviewer_override_from_config(
    config: &Config,
) -> Option<praxis_app_gateway_protocol::ApprovalsReviewer> {
    Some(config.approvals_reviewer.into())
}

fn config_request_overrides_from_config(
    config: &Config,
) -> Option<HashMap<String, serde_json::Value>> {
    config.active_profile.as_ref().map(|profile| {
        HashMap::from([(
            "profile".to_string(),
            serde_json::Value::String(profile.clone()),
        )])
    })
}

fn sandbox_mode_from_policy(
    policy: SandboxPolicy,
) -> Option<praxis_app_gateway_protocol::SandboxMode> {
    match policy {
        SandboxPolicy::DangerFullAccess => {
            Some(praxis_app_gateway_protocol::SandboxMode::DangerFullAccess)
        }
        SandboxPolicy::ReadOnly { .. } => Some(praxis_app_gateway_protocol::SandboxMode::ReadOnly),
        SandboxPolicy::WorkspaceWrite { .. } => {
            Some(praxis_app_gateway_protocol::SandboxMode::WorkspaceWrite)
        }
        SandboxPolicy::ExternalSandbox { .. } => None,
    }
}

fn thread_start_params_from_config(
    config: &Config,
    thread_params_mode: ThreadParamsMode,
) -> ThreadStartParams {
    ThreadStartParams {
        model: config.model.clone(),
        model_provider: thread_params_mode.model_provider_from_config(config),
        cwd: thread_cwd_from_config(config, thread_params_mode),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get().clone()),
        config: config_request_overrides_from_config(config),
        ephemeral: Some(config.ephemeral),
        persist_extended_history: true,
        ..ThreadStartParams::default()
    }
}

fn thread_resume_params_from_config(
    config: Config,
    thread_id: ThreadId,
    thread_params_mode: ThreadParamsMode,
) -> ThreadResumeParams {
    ThreadResumeParams {
        thread_id: thread_id.to_string(),
        model: config.model.clone(),
        model_provider: thread_params_mode.model_provider_from_config(&config),
        cwd: thread_cwd_from_config(&config, thread_params_mode),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(&config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get().clone()),
        config: config_request_overrides_from_config(&config),
        persist_extended_history: true,
        ..ThreadResumeParams::default()
    }
}

fn thread_fork_params_from_config(
    config: Config,
    thread_id: ThreadId,
    path: Option<PathBuf>,
    thread_params_mode: ThreadParamsMode,
) -> ThreadForkParams {
    ThreadForkParams {
        thread_id: thread_id.to_string(),
        path,
        model: config.model.clone(),
        model_provider: thread_params_mode.model_provider_from_config(&config),
        cwd: thread_cwd_from_config(&config, thread_params_mode),
        approval_policy: Some(config.permissions.approval_policy.value().into()),
        approvals_reviewer: approvals_reviewer_override_from_config(&config),
        sandbox: sandbox_mode_from_policy(config.permissions.sandbox_policy.get().clone()),
        config: config_request_overrides_from_config(&config),
        ephemeral: config.ephemeral,
        persist_extended_history: true,
        ..ThreadForkParams::default()
    }
}

fn thread_cwd_from_config(config: &Config, thread_params_mode: ThreadParamsMode) -> Option<String> {
    match thread_params_mode {
        ThreadParamsMode::Embedded => Some(config.cwd.to_string_lossy().to_string()),
        ThreadParamsMode::Remote => None,
    }
}

async fn started_thread_from_start_response(
    response: ThreadStartResponse,
    config: &Config,
) -> Result<AppGatewayStartedThread> {
    let session = thread_session_state_from_thread_start_response(&response, config)
        .await
        .map_err(color_eyre::eyre::Report::msg)?;
    Ok(AppGatewayStartedThread {
        session,
        turns: response.thread.turns,
        status: response.thread.status,
        control_state: response.thread.control_state,
    })
}

async fn started_thread_from_resume_response(
    response: ThreadResumeResponse,
    config: &Config,
) -> Result<AppGatewayStartedThread> {
    let session = thread_session_state_from_thread_resume_response(&response, config)
        .await
        .map_err(color_eyre::eyre::Report::msg)?;
    Ok(AppGatewayStartedThread {
        session,
        turns: response.thread.turns,
        status: response.thread.status,
        control_state: response.thread.control_state,
    })
}

async fn started_thread_from_fork_response(
    response: ThreadForkResponse,
    config: &Config,
) -> Result<AppGatewayStartedThread> {
    let session = thread_session_state_from_thread_fork_response(&response, config)
        .await
        .map_err(color_eyre::eyre::Report::msg)?;
    Ok(AppGatewayStartedThread {
        session,
        turns: response.thread.turns,
        status: response.thread.status,
        control_state: response.thread.control_state,
    })
}

async fn thread_session_state_from_thread_start_response(
    response: &ThreadStartResponse,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    thread_session_state_from_thread_response(
        &response.thread.id,
        response.thread.name.clone(),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort,
        response.thread.selfwork_plan_path.clone(),
        config,
    )
    .await
}

async fn thread_session_state_from_thread_resume_response(
    response: &ThreadResumeResponse,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    thread_session_state_from_thread_response(
        &response.thread.id,
        response.thread.name.clone(),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort,
        response.thread.selfwork_plan_path.clone(),
        config,
    )
    .await
}

async fn thread_session_state_from_thread_fork_response(
    response: &ThreadForkResponse,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    thread_session_state_from_thread_response(
        &response.thread.id,
        response.thread.name.clone(),
        response.thread.path.clone(),
        response.model.clone(),
        response.model_provider.clone(),
        response.service_tier,
        response.approval_policy.to_core(),
        response.approvals_reviewer.to_core(),
        response.sandbox.to_core(),
        response.cwd.clone(),
        response.reasoning_effort,
        response.thread.selfwork_plan_path.clone(),
        config,
    )
    .await
}

fn review_target_to_app_gateway(
    target: CoreReviewTarget,
) -> praxis_app_gateway_protocol::ReviewTarget {
    match target {
        CoreReviewTarget::UncommittedChanges => {
            praxis_app_gateway_protocol::ReviewTarget::UncommittedChanges
        }
        CoreReviewTarget::BaseBranch { branch } => {
            praxis_app_gateway_protocol::ReviewTarget::BaseBranch { branch }
        }
        CoreReviewTarget::Commit { sha, title } => {
            praxis_app_gateway_protocol::ReviewTarget::Commit { sha, title }
        }
        CoreReviewTarget::Custom { instructions } => {
            praxis_app_gateway_protocol::ReviewTarget::Custom { instructions }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "session mapping keeps explicit fields"
)]
async fn thread_session_state_from_thread_response(
    thread_id: &str,
    thread_name: Option<String>,
    rollout_path: Option<PathBuf>,
    model: String,
    model_provider_id: String,
    service_tier: Option<praxis_protocol::config_types::ServiceTier>,
    approval_policy: AskForApproval,
    approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
    sandbox_policy: SandboxPolicy,
    cwd: PathBuf,
    reasoning_effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
    selfwork_plan_path: Option<PathBuf>,
    config: &Config,
) -> Result<ThreadSessionState, String> {
    let thread_id = ThreadId::from_string(thread_id)
        .map_err(|err| format!("thread id `{thread_id}` is invalid: {err}"))?;
    let (history_log_id, history_entry_count) = message_history::history_metadata(config).await;
    let history_entry_count = u64::try_from(history_entry_count).unwrap_or(u64::MAX);

    Ok(ThreadSessionState {
        thread_id,
        forked_from_id: None,
        thread_name,
        model,
        model_provider_id,
        service_tier,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        cwd,
        reasoning_effort,
        history_log_id,
        history_entry_count,
        network_proxy: None,
        rollout_path,
        selfwork_plan_path,
    })
}

pub(crate) fn app_gateway_rate_limit_snapshots_to_core(
    response: GetAccountRateLimitsResponse,
) -> Vec<RateLimitSnapshot> {
    response
        .rate_limits
        .into_values()
        .map(app_gateway_rate_limit_snapshot_to_core)
        .collect()
}

pub(crate) fn app_gateway_rate_limit_snapshot_to_core(
    snapshot: praxis_app_gateway_protocol::RateLimitSnapshot,
) -> RateLimitSnapshot {
    RateLimitSnapshot {
        limit_id: snapshot.limit_id,
        limit_name: snapshot.limit_name,
        primary: snapshot.primary.map(app_gateway_rate_limit_window_to_core),
        secondary: snapshot
            .secondary
            .map(app_gateway_rate_limit_window_to_core),
        credits: snapshot.credits.map(app_gateway_credits_snapshot_to_core),
        plan_type: snapshot.plan_type,
    }
}

fn app_gateway_rate_limit_window_to_core(
    window: praxis_app_gateway_protocol::RateLimitWindow,
) -> RateLimitWindow {
    RateLimitWindow {
        used_percent: window.used_percent as f64,
        window_minutes: window.window_duration_mins,
        resets_at: window.resets_at,
    }
}

fn app_gateway_credits_snapshot_to_core(
    snapshot: praxis_app_gateway_protocol::CreditsSnapshot,
) -> CreditsSnapshot {
    CreditsSnapshot {
        has_credits: snapshot.has_credits,
        unlimited: snapshot.unlimited,
        balance: snapshot.balance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_app_gateway_protocol::ThreadStatus;
    use praxis_app_gateway_protocol::Turn;
    use praxis_app_gateway_protocol::TurnStatus;
    use praxis_core::config::ConfigBuilder;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    async fn build_config(temp_dir: &TempDir) -> Config {
        ConfigBuilder::default()
            .praxis_home(temp_dir.path().to_path_buf())
            .build()
            .await
            .expect("config should build")
    }

    #[tokio::test]
    async fn thread_start_params_include_cwd_for_embedded_sessions() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;

        let params = thread_start_params_from_config(&config, ThreadParamsMode::Embedded);

        assert_eq!(params.cwd, Some(config.cwd.to_string_lossy().to_string()));
        assert_eq!(params.model_provider, Some(config.model_provider_id));
    }

    #[tokio::test]
    async fn thread_lifecycle_params_omit_local_overrides_for_remote_sessions() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();

        let start = thread_start_params_from_config(&config, ThreadParamsMode::Remote);
        let resume =
            thread_resume_params_from_config(config.clone(), thread_id, ThreadParamsMode::Remote);
        let fork = thread_fork_params_from_config(
            config,
            thread_id,
            /*path*/ None,
            ThreadParamsMode::Remote,
        );

        assert_eq!(start.cwd, None);
        assert_eq!(resume.cwd, None);
        assert_eq!(fork.cwd, None);
        assert_eq!(start.model_provider, None);
        assert_eq!(resume.model_provider, None);
        assert_eq!(fork.model_provider, None);
    }

    #[tokio::test]
    async fn resume_response_restores_turns_from_thread_items() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();
        let response = ThreadResumeResponse {
            thread: praxis_app_gateway_protocol::Thread {
                id: thread_id.to_string(),
                preview: "hello".to_string(),
                summary: None,
                ephemeral: false,
                model_provider: "openai".to_string(),
                model: Some("gpt-5.4".to_string()),
                created_at: 1,
                updated_at: 2,
                status: ThreadStatus::Idle,
                path: None,
                cwd: PathBuf::from("/tmp/project"),
                cli_version: "0.0.0".to_string(),
                source: praxis_protocol::protocol::SessionSource::Cli.into(),
                agent_display_name: None,
                agent_role: None,
                git_info: None,
                name: None,
                total_cost_usd: None,
                last_cost_usd: None,
                token_usage: None,
                control_state: None,
                selfwork_plan_path: None,
                turns: vec![Turn {
                    id: "turn-1".to_string(),
                    items: vec![
                        praxis_app_gateway_protocol::ThreadItem::UserMessage {
                            id: "user-1".to_string(),
                            content: vec![praxis_app_gateway_protocol::UserInput::Text {
                                text: "hello from history".to_string(),
                                text_elements: Vec::new(),
                            }],
                        },
                        praxis_app_gateway_protocol::ThreadItem::AgentMessage {
                            id: "assistant-1".to_string(),
                            text: "assistant reply".to_string(),
                            phase: None,
                            memory_citation: None,
                        },
                    ],
                    status: TurnStatus::Completed,
                    error: None,
                }],
            },
            model: "gpt-5.4".to_string(),
            model_provider: "openai".to_string(),
            service_tier: None,
            cwd: PathBuf::from("/tmp/project"),
            approval_policy: praxis_protocol::protocol::AskForApproval::Never.into(),
            approvals_reviewer: praxis_app_gateway_protocol::ApprovalsReviewer::User,
            sandbox: praxis_protocol::protocol::SandboxPolicy::new_read_only_policy().into(),
            reasoning_effort: None,
        };

        let started = started_thread_from_resume_response(response.clone(), &config)
            .await
            .expect("resume response should map");
        assert_eq!(started.turns.len(), 1);
        assert_eq!(started.turns[0], response.thread.turns[0]);
    }

    #[tokio::test]
    async fn session_configured_populates_history_metadata() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let config = build_config(&temp_dir).await;
        let thread_id = ThreadId::new();

        message_history::append_entry("older", &thread_id, &config)
            .await
            .expect("history append should succeed");
        message_history::append_entry("newer", &thread_id, &config)
            .await
            .expect("history append should succeed");

        let session = thread_session_state_from_thread_response(
            &thread_id.to_string(),
            Some("restore".to_string()),
            /*rollout_path*/ None,
            "gpt-5.4".to_string(),
            "openai".to_string(),
            /*service_tier*/ None,
            AskForApproval::Never,
            praxis_protocol::config_types::ApprovalsReviewer::User,
            SandboxPolicy::new_read_only_policy(),
            PathBuf::from("/tmp/project"),
            /*reasoning_effort*/ None,
            /*selfwork_plan_path*/ None,
            &config,
        )
        .await
        .expect("session should map");

        assert_ne!(session.history_log_id, 0);
        assert_eq!(session.history_entry_count, 2);
    }

    #[test]
    fn status_account_display_from_auth_mode_uses_remapped_plan_labels() {
        let business = status_account_display_from_auth_mode(
            Some(AuthMode::Chatgpt),
            Some(praxis_protocol::account::PlanType::EnterpriseCbpUsageBased),
        );
        assert!(matches!(
            business,
            Some(StatusAccountDisplay::ChatGpt {
                email: None,
                plan: Some(ref plan),
            }) if plan == "Enterprise"
        ));

        let team = status_account_display_from_auth_mode(
            Some(AuthMode::Chatgpt),
            Some(praxis_protocol::account::PlanType::SelfServeBusinessUsageBased),
        );
        assert!(matches!(
            team,
            Some(StatusAccountDisplay::ChatGpt {
                email: None,
                plan: Some(ref plan),
            }) if plan == "Business"
        ));
    }
}
