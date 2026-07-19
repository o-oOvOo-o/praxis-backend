//! Turn-loop bridge construction, shared context, input, and state.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter) mod bridge {
    use tokio_util::sync::CancellationToken;

    use super::hooks::PraxisTurnHooks;
    use super::services::PraxisTurnServices;

    pub(in crate::praxis) struct PraxisTurnLoopBridge {
        pub(in crate::praxis::turn_loop_adapter) ctx: praxis_loop::TurnContext,
        pub(in crate::praxis::turn_loop_adapter) input: praxis_loop::TurnInput,
        pub(in crate::praxis::turn_loop_adapter) state: praxis_loop::TurnState,
        pub(in crate::praxis::turn_loop_adapter) services: PraxisTurnServices,
        pub(in crate::praxis::turn_loop_adapter) hooks: PraxisTurnHooks,
        pub(in crate::praxis::turn_loop_adapter) cancellation_token: CancellationToken,
    }

    pub(in crate::praxis) enum PraxisTurnLoopOutcome {
        Complete { last_agent_message: Option<String> },
        WantsFollowup { last_agent_message: Option<String> },
        Aborted { reason: PraxisTurnLoopAbort },
    }

    pub(in crate::praxis) struct PraxisTurnLoopAbort {
        pub(in crate::praxis) message: String,
        pub(in crate::praxis) cancelled: bool,
    }

    impl PraxisTurnLoopAbort {
        fn from_loop_error(error: praxis_loop::TurnError) -> Self {
            Self {
                message: error.message,
                cancelled: error.kind == praxis_loop::TurnErrorKind::Cancelled,
            }
        }
    }

    impl PraxisTurnLoopBridge {
        pub(in crate::praxis) async fn run(self) -> PraxisTurnLoopOutcome {
            let Self {
                ctx,
                input,
                state,
                services,
                hooks,
                cancellation_token,
            } = self;
            let result =
                praxis_loop::run_turn(ctx, state, &services, &hooks, input, cancellation_token)
                    .await;

            match result {
                praxis_loop::TurnResult::Complete { state } => PraxisTurnLoopOutcome::Complete {
                    last_agent_message: state
                        .into_last_agent_message()
                        .or(services.last_agent_message().await),
                },
                praxis_loop::TurnResult::WantsFollowup { state } => {
                    PraxisTurnLoopOutcome::WantsFollowup {
                        last_agent_message: state
                            .into_last_agent_message()
                            .or(services.last_agent_message().await),
                    }
                }
                praxis_loop::TurnResult::Aborted { reason, .. } => PraxisTurnLoopOutcome::Aborted {
                    reason: PraxisTurnLoopAbort::from_loop_error(reason),
                },
            }
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod builder {
    use std::sync::Arc;

    use praxis_protocol::user_input::UserInput;
    use tokio_util::sync::CancellationToken;

    use crate::client::ModelClientSession;

    use super::super::Session;
    use super::super::TurnContext;
    use super::bridge::PraxisTurnLoopBridge;
    use super::context;
    use super::hooks::PraxisTurnHooks;
    use super::input_projection;
    use super::prompt_bridge;
    use super::services::PraxisTurnServices;
    use super::state::PraxisTurnBridgeState;

    pub(in crate::praxis) struct PraxisTurnLoopAdapter;

    impl PraxisTurnLoopAdapter {
        pub(in crate::praxis) async fn build_bridge(
            sess: Arc<Session>,
            turn_context: Arc<TurnContext>,
            input: &[UserInput],
            prewarmed_client_session: Option<ModelClientSession>,
            cancellation_token: CancellationToken,
        ) -> PraxisTurnLoopBridge {
            let bridge_state = Arc::new(PraxisTurnBridgeState::new(
                input_projection::model_request_messages(input),
            ));
            let initial_prompt_items =
                prompt_bridge::initial_prompt_items_from_session_history(&sess, &turn_context)
                    .await;

            PraxisTurnLoopBridge {
                ctx: context::build_context(
                    sess.as_ref(),
                    turn_context.as_ref(),
                    initial_prompt_items,
                ),
                input: prompt_bridge::input_to_turn_input(input),
                state: praxis_loop::TurnState::default(),
                services: PraxisTurnServices::new(
                    Arc::clone(&sess),
                    Arc::clone(&turn_context),
                    Arc::clone(&bridge_state),
                    prewarmed_client_session,
                ),
                hooks: PraxisTurnHooks::new(
                    sess,
                    turn_context,
                    input.to_vec(),
                    bridge_state,
                    cancellation_token.clone(),
                ),
                cancellation_token,
            }
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod context {
    use super::super::Session;
    use super::super::TurnContext;

    mod collaboration {
        use praxis_protocol::config_types::ModeKind;

        use super::super::super::TurnContext;

        pub(in crate::praxis::turn_loop_adapter) fn build_collaboration_mode(
            turn_context: &TurnContext,
        ) -> praxis_loop::context::CollaborationMode {
            match turn_context.collaboration_mode.mode {
                ModeKind::Plan => praxis_loop::context::CollaborationMode::ReadOnly,
                ModeKind::Default | ModeKind::PairProgramming | ModeKind::Execute => {
                    praxis_loop::context::CollaborationMode::FullAccess
                }
            }
        }
    }
    mod model {
        use super::super::super::TurnContext;

        pub(in crate::praxis::turn_loop_adapter) fn build_model_spec(
            turn_context: &TurnContext,
        ) -> praxis_loop::model::ModelSpec {
            praxis_loop::model::ModelSpec {
                slug: turn_context.model_info.slug.clone(),
                provider_id: Some(turn_context.config.model_provider_id.clone()),
                context_window: loop_context_window(turn_context.model_context_window()),
                input_modalities: turn_context
                    .model_info
                    .input_modalities
                    .iter()
                    .map(|modality| format!("{modality:?}"))
                    .collect(),
            }
        }

        fn loop_context_window(value: Option<i64>) -> Option<u64> {
            let Some(value) = value else {
                return None;
            };
            match u64::try_from(value) {
                Ok(value) => Some(value),
                Err(_) => None,
            }
        }
    }
    mod permissions {
        use praxis_protocol::protocol::AskForApproval;
        use praxis_protocol::protocol::SandboxPolicy;

        use super::super::super::TurnContext;

        pub(in crate::praxis::turn_loop_adapter) fn build_permissions(
            turn_context: &TurnContext,
        ) -> praxis_loop::context::EffectivePermissions {
            let permissions = turn_context.effective_permissions();
            let sandbox_policy = permissions.sandbox_policy.get();
            praxis_loop::context::EffectivePermissions {
                write: sandbox_policy.has_full_disk_write_access()
                    || matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }),
                network: sandbox_policy.has_full_network_access(),
                approval_required: !matches!(
                    permissions.approval_policy.value(),
                    AskForApproval::Never
                ),
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn build_context(
        sess: &Session,
        turn_context: &TurnContext,
        initial_prompt_items: Vec<praxis_loop::model::PromptItem>,
    ) -> praxis_loop::TurnContext {
        let mut ctx = praxis_loop::TurnContext::new(
            praxis_loop::ids::TurnId::new(turn_context.sub_id.clone()),
            praxis_loop::ids::ThreadId::new(sess.conversation_id.to_string()),
            praxis_loop::ids::TraceId::new(turn_context.trace_id.clone().unwrap_or_default()),
            model::build_model_spec(turn_context),
        );
        ctx.reasoning = turn_context
            .reasoning_effort
            .clone()
            .map(|reasoning_effort| reasoning_effort.to_string());
        ctx.service_tier = turn_context
            .config
            .service_tier
            .map(|service_tier| service_tier.to_string());
        ctx.permissions = permissions::build_permissions(turn_context);
        ctx.collaboration_mode = collaboration::build_collaboration_mode(turn_context);
        ctx.cwd = Some(turn_context.cwd.to_path_buf());
        ctx.features = praxis_loop::context::TurnFeatures {
            streaming: true,
            tool_calls: true,
        };
        ctx.initial_prompt_items = initial_prompt_items;
        ctx
    }
}

pub(in crate::praxis::turn_loop_adapter) mod input_projection {
    use praxis_protocol::user_input::UserInput;

    pub(in crate::praxis::turn_loop_adapter) fn model_request_messages(
        input: &[UserInput],
    ) -> Vec<String> {
        input
            .iter()
            .map(|item| match item {
                UserInput::Text { text, .. } => text.clone(),
                UserInput::Image { image_url } => format!("[image: {image_url}]"),
                UserInput::LocalImage { path } => format!("[local image: {}]", path.display()),
                UserInput::Skill { name, path } => format!("[skill: {name} at {}]", path.display()),
                UserInput::Mention { name, path } => format!("[mention: {name} at {path}]"),
                _ => format!("{item:?}"),
            })
            .collect()
    }
}

pub(in crate::praxis::turn_loop_adapter) mod round_input {
    use praxis_protocol::items::TurnItem;
    use praxis_protocol::models::ResponseItem;

    use crate::event_mapping::parse_turn_item;

    use super::super::TurnContext;
    use super::prompt_bridge;

    #[derive(Debug)]
    pub(in crate::praxis::turn_loop_adapter) struct PraxisRoundInput {
        pub(in crate::praxis::turn_loop_adapter) items: Vec<ResponseItem>,
        pub(in crate::praxis::turn_loop_adapter) user_messages: Vec<String>,
        pub(in crate::praxis::turn_loop_adapter) turn_metadata_header: Option<String>,
    }

    pub(in crate::praxis::turn_loop_adapter) fn build_round_input(
        turn_context: &TurnContext,
        prompt_items: &[praxis_loop::model::PromptItem],
    ) -> PraxisRoundInput {
        let items = prompt_bridge::response_items_from_prompt_items(prompt_items);
        let user_messages = round_user_messages(&items);
        let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();

        PraxisRoundInput {
            items,
            user_messages,
            turn_metadata_header,
        }
    }

    fn round_user_messages(input: &[ResponseItem]) -> Vec<String> {
        let mut messages = Vec::new();
        for item in input {
            match round_item_projection(item) {
                RoundItemProjection::UserMessage(message) => messages.push(message),
                RoundItemProjection::NonUser => {}
            }
        }
        messages
    }

    enum RoundItemProjection {
        UserMessage(String),
        NonUser,
    }

    fn round_item_projection(item: &ResponseItem) -> RoundItemProjection {
        match parse_turn_item(item) {
            Some(TurnItem::UserMessage(user_message)) => {
                RoundItemProjection::UserMessage(user_message.message())
            }
            _ => RoundItemProjection::NonUser,
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod state {
    use std::collections::HashSet;

    use praxis_loop::outcome::TurnCompletionMessage;
    use tokio::sync::Mutex;

    use super::prepare_phase::TurnPrepareOutcome;

    #[derive(Debug)]
    pub(in crate::praxis::turn_loop_adapter) struct PraxisTurnBridgeState {
        explicitly_enabled_connectors: Mutex<HashSet<String>>,
        model_request_input_messages: Mutex<Vec<String>>,
        stop_hook_active: Mutex<bool>,
        last_agent_message: Mutex<Option<String>>,
    }

    impl PraxisTurnBridgeState {
        pub(in crate::praxis::turn_loop_adapter) fn new(
            model_request_input_messages: Vec<String>,
        ) -> Self {
            Self {
                explicitly_enabled_connectors: Mutex::new(HashSet::new()),
                model_request_input_messages: Mutex::new(model_request_input_messages),
                stop_hook_active: Mutex::new(false),
                last_agent_message: Mutex::new(None),
            }
        }

        pub(in crate::praxis::turn_loop_adapter) async fn apply_prepare_outcome(
            &self,
            outcome: TurnPrepareOutcome,
        ) {
            self.set_explicitly_enabled_connectors(outcome.explicitly_enabled_connectors)
                .await;
        }

        async fn set_explicitly_enabled_connectors(&self, connectors: HashSet<String>) {
            *self.explicitly_enabled_connectors.lock().await = connectors;
        }

        pub(in crate::praxis::turn_loop_adapter) async fn explicitly_enabled_connectors(
            &self,
        ) -> HashSet<String> {
            self.explicitly_enabled_connectors.lock().await.clone()
        }

        pub(in crate::praxis::turn_loop_adapter) async fn set_model_request_input_messages(
            &self,
            messages: Vec<String>,
        ) {
            *self.model_request_input_messages.lock().await = messages;
        }

        pub(in crate::praxis::turn_loop_adapter) async fn model_request_input_messages(
            &self,
        ) -> Vec<String> {
            self.model_request_input_messages.lock().await.clone()
        }

        pub(in crate::praxis::turn_loop_adapter) async fn record_agent_message(
            &self,
            message: impl Into<String>,
        ) {
            self.set_last_agent_message(Some(message.into())).await;
        }

        pub(in crate::praxis::turn_loop_adapter) async fn record_completion_message(
            &self,
            message: &TurnCompletionMessage,
        ) {
            self.set_last_agent_message(message.clone().into_option())
                .await;
        }

        pub(in crate::praxis::turn_loop_adapter) async fn record_optional_agent_message(
            &self,
            message: Option<String>,
        ) {
            self.set_last_agent_message(message).await;
        }

        pub(in crate::praxis::turn_loop_adapter) async fn last_agent_message(
            &self,
        ) -> Option<String> {
            self.last_agent_message.lock().await.clone()
        }

        pub(in crate::praxis::turn_loop_adapter) async fn stop_hook_active(&self) -> bool {
            *self.stop_hook_active.lock().await
        }

        pub(in crate::praxis::turn_loop_adapter) async fn set_stop_hook_active(
            &self,
            active: bool,
        ) {
            *self.stop_hook_active.lock().await = active;
        }

        async fn set_last_agent_message(&self, message: Option<String>) {
            *self.last_agent_message.lock().await = message;
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod model_round_state {
    use std::sync::Arc;

    use crate::client::ModelClientSession;
    use crate::tools::context::SharedTurnDiffTracker;
    use crate::turn_diff_tracker::TurnDiffTracker;

    use super::super::Session;
    use super::super::TurnContext;

    pub(in crate::praxis::turn_loop_adapter) struct PraxisModelRoundState {
        turn_diff_tracker: SharedTurnDiffTracker,
        client_session: ModelClientSession,
        server_model_warning_emitted_for_turn: bool,
    }

    impl PraxisModelRoundState {
        pub(in crate::praxis::turn_loop_adapter) fn new(
            sess: &Session,
            turn_context: &TurnContext,
            prewarmed_client_session: Option<ModelClientSession>,
        ) -> Self {
            let client_session = match prewarmed_client_session {
                Some(client_session)
                    if client_session.matches_provider(
                        &turn_context.config.model_provider_id,
                        &turn_context.provider,
                    ) =>
                {
                    client_session
                }
                Some(_) | None => sess.services.model_runtime.new_session_for(
                    &turn_context.config.model_provider_id,
                    &turn_context.provider,
                ),
            };

            Self {
                turn_diff_tracker: Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new())),
                client_session,
                server_model_warning_emitted_for_turn: false,
            }
        }

        pub(in crate::praxis::turn_loop_adapter) fn turn_diff_tracker(
            &self,
        ) -> SharedTurnDiffTracker {
            Arc::clone(&self.turn_diff_tracker)
        }

        pub(in crate::praxis::turn_loop_adapter) fn client_session_mut(
            &mut self,
        ) -> &mut ModelClientSession {
            &mut self.client_session
        }

        pub(in crate::praxis::turn_loop_adapter) fn server_model_warning_emitted_for_turn_mut(
            &mut self,
        ) -> &mut bool {
            &mut self.server_model_warning_emitted_for_turn
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) mod tool_runtime_slot {
    use std::sync::Arc;
    use std::sync::RwLock;

    use praxis_loop::outcome::LoopResult;
    use praxis_loop::outcome::TurnError;
    use praxis_loop::outcome::TurnErrorKind;

    use crate::tools::tool_call_runtime::ToolCallRuntime;

    #[derive(Clone, Default)]
    pub(in crate::praxis::turn_loop_adapter) struct ModelRoundToolsSlot {
        inner: Arc<RwLock<Option<ToolCallRuntime>>>,
    }

    impl ModelRoundToolsSlot {
        pub(in crate::praxis::turn_loop_adapter) fn store(
            &self,
            runtime: ToolCallRuntime,
        ) -> LoopResult<()> {
            let mut guard = self.inner.write().map_err(|_| lock_error())?;
            *guard = Some(runtime);
            Ok(())
        }

        pub(in crate::praxis::turn_loop_adapter) fn current(&self) -> Option<ToolCallRuntime> {
            self.inner.read().ok()?.as_ref().cloned()
        }
    }

    fn lock_error() -> TurnError {
        TurnError::new(
            TurnErrorKind::Internal,
            "round tool runtime state lock was poisoned",
        )
    }
}
