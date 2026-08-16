//! One model round, tool preparation, and round-local failures.

#![allow(unused_imports)]

use super::*;

pub(in crate::praxis::turn_loop_adapter::model_stream) mod model_round {
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::services::ModelRequest;
    use tokio_util::sync::CancellationToken;

    use crate::client_common::Prompt;
    use crate::tools::code_mode::CodeModeTurnWorker;

    use super::PraxisModelStreamInput;
    use super::request_context;

    mod input_projection {

        use praxis_loop::services::ModelRequest;

        use super::super::super::round_input;

        use super::super::super::round_input::PraxisRoundInput;

        use super::super::PraxisModelStreamInput;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn project_round_input(
            input: &PraxisModelStreamInput,

            request: &ModelRequest,
        ) -> PraxisRoundInput {
            let round_input = round_input::build_round_input(&input.turn_context, &request.prompt);

            input
                .bridge_state
                .set_model_request_input_messages(round_input.user_messages.clone())
                .await;

            round_input
        }
    }
    mod prompt {
        use praxis_protocol::models::ResponseItem;

        use crate::client_common::Prompt;
        use crate::praxis::Session;
        use crate::praxis::TurnContext;
        use crate::tools::ToolRouter;

        use super::super::super::super::model_request::build_prompt;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn build_provider_prompt(
            session: &Session,
            turn_context: &TurnContext,
            items: Vec<ResponseItem>,
            router: &ToolRouter,
        ) -> Prompt {
            let base_instructions = session.get_base_instructions().await;
            let result = build_prompt(items, router, turn_context, base_instructions);
            for event in result.saving_events {
                session.record_token_saving_event(turn_context, event).await;
            }
            result.prompt
        }
    }
    mod tool_runtime {
        use std::collections::HashSet;
        use std::sync::Arc;

        use praxis_protocol::models::ResponseItem;
        use tokio_util::sync::CancellationToken;

        use crate::SkillLoadOutcome;
        use crate::capabilities::ToolCapabilities;
        use crate::capabilities::ToolCapability;
        use crate::error::Result as PraxisResult;
        use crate::praxis::Session;
        use crate::praxis::TurnContext;
        use crate::tools::context::SharedTurnDiffTracker;
        use crate::tools::tool_call_runtime::ToolCallRuntime;

        use super::super::super::super::model_request::built_tools;

        pub(in crate::praxis::turn_loop_adapter::model_stream) struct ModelRoundTools {
            routers: ToolCapabilities,
            runtime: ToolCallRuntime,
        }

        impl ModelRoundTools {
            pub(in crate::praxis::turn_loop_adapter::model_stream) fn router(
                &self,
            ) -> &crate::tools::ToolRouter {
                self.routers.as_ref()
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn code_mode_router(
                &self,
            ) -> ToolCapability {
                self.routers.code_mode()
            }

            pub(in crate::praxis::turn_loop_adapter::model_stream) fn runtime(
                &self,
            ) -> ToolCallRuntime {
                self.runtime.clone()
            }
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn build_tool_runtime(
            sess: Arc<Session>,
            turn_context: Arc<TurnContext>,
            turn_diff_tracker: SharedTurnDiffTracker,
            input: &[ResponseItem],
            explicitly_enabled_connectors: &HashSet<String>,
            skills_outcome: Option<&SkillLoadOutcome>,
            cancellation_token: &CancellationToken,
        ) -> PraxisResult<ModelRoundTools> {
            let routers = built_tools(
                sess.as_ref(),
                turn_context.as_ref(),
                input,
                explicitly_enabled_connectors,
                skills_outcome,
                cancellation_token,
            )
            .await?;
            let runtime = ToolCallRuntime::new(
                routers.model(),
                routers.code_mode(),
                Arc::clone(&sess),
                Arc::clone(&turn_context),
                Arc::clone(&turn_diff_tracker),
            );

            Ok(ModelRoundTools { routers, runtime })
        }
    }
    mod tooling {
        use std::sync::Arc;

        use praxis_loop::outcome::LoopResult;
        use praxis_protocol::models::ResponseItem;
        use tokio_util::sync::CancellationToken;

        use crate::client_common::Prompt;
        use crate::tools::code_mode::CodeModeTurnWorker;

        use super::super::PraxisModelStreamInput;
        use super::super::code_mode_worker;
        use super::prompt;
        use super::tools;

        pub(in crate::praxis::turn_loop_adapter::model_stream) struct PreparedTooling {
            pub(in crate::praxis::turn_loop_adapter::model_stream) prompt: Prompt,
            pub(in crate::praxis::turn_loop_adapter::model_stream) code_mode_worker:
                Option<CodeModeTurnWorker>,
        }

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn prepare_tooling(
            input: &PraxisModelStreamInput,
            items: Vec<ResponseItem>,
            cancellation_token: &CancellationToken,
        ) -> LoopResult<PreparedTooling> {
            let explicitly_enabled_connectors =
                input.bridge_state.explicitly_enabled_connectors().await;
            let turn_diff_tracker = input.runtime_state.lock().await.turn_diff_tracker();
            let tools = tools::build_tools(
                &input.session,
                &input.turn_context,
                Arc::clone(&turn_diff_tracker),
                &items,
                &explicitly_enabled_connectors,
                cancellation_token,
            )
            .await?;

            let prompt = prompt::build_provider_prompt(
                input.session.as_ref(),
                input.turn_context.as_ref(),
                items,
                tools.router(),
            )
            .await;

            input.tool_runtime_slot.store(tools.runtime())?;

            let code_mode_worker = code_mode_worker::start_turn_worker(
                &input.session,
                &input.turn_context,
                tools.code_mode_router(),
                Arc::clone(&turn_diff_tracker),
            )
            .await;

            Ok(PreparedTooling {
                prompt,
                code_mode_worker,
            })
        }
    }
    mod tools {
        use std::collections::HashSet;
        use std::sync::Arc;

        use praxis_loop::outcome::LoopResult;
        use praxis_protocol::models::ResponseItem;
        use tokio_util::sync::CancellationToken;

        use crate::praxis::Session;
        use crate::praxis::TurnContext;
        use crate::tools::context::SharedTurnDiffTracker;

        use super::super::error_bridge::model_error;
        use super::tool_runtime::ModelRoundTools;
        use super::tool_runtime::build_tool_runtime;

        pub(in crate::praxis::turn_loop_adapter::model_stream) async fn build_tools(
            session: &Arc<Session>,
            turn_context: &Arc<TurnContext>,
            turn_diff_tracker: SharedTurnDiffTracker,
            items: &[ResponseItem],
            explicitly_enabled_connectors: &HashSet<String>,
            cancellation_token: &CancellationToken,
        ) -> LoopResult<ModelRoundTools> {
            build_tool_runtime(
                Arc::clone(session),
                Arc::clone(turn_context),
                turn_diff_tracker,
                items,
                explicitly_enabled_connectors,
                Some(turn_context.turn_skills.outcome.as_ref()),
                cancellation_token,
            )
            .await
            .map_err(model_error)
        }
    }
    pub(in crate::praxis::turn_loop_adapter::model_stream) struct PreparedModelRound {
        pub(in crate::praxis::turn_loop_adapter::model_stream) input: PraxisModelStreamInput,
        pub(in crate::praxis::turn_loop_adapter::model_stream) prompt: Prompt,
        pub(in crate::praxis::turn_loop_adapter::model_stream) turn_metadata_header: Option<String>,
        pub(in crate::praxis::turn_loop_adapter::model_stream) code_mode_worker:
            Option<CodeModeTurnWorker>,
    }

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn prepare_model_round(
        mut input: PraxisModelStreamInput,
        request: ModelRequest,
        cancellation_token: &CancellationToken,
    ) -> LoopResult<PreparedModelRound> {
        input.turn_context = request_context::resolve_request_turn_context(
            &input.session,
            &input.turn_context,
            &request,
        )
        .await?;
        tracing::trace!(
            round = request.round,
            model = input.turn_context.model_info.slug.as_str(),
            reasoning = ?input.turn_context.reasoning_effort,
            service_tier = ?input.turn_context.config.service_tier,
            loop_prompt_items = request.prompt.len(),
            "building Praxis provider prompt from loop request"
        );
        let round_input = input_projection::project_round_input(&input, &request).await;

        let tooling =
            tooling::prepare_tooling(&input, round_input.items, cancellation_token).await?;

        Ok(PreparedModelRound {
            input,
            prompt: tooling.prompt,
            turn_metadata_header: round_input.turn_metadata_header,
            code_mode_worker: tooling.code_mode_worker,
        })
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod code_mode_worker {
    use std::sync::Arc;

    use crate::capabilities::ToolCapability;
    use crate::praxis::Session;
    use crate::praxis::TurnContext;
    use crate::tools::code_mode::CodeModeTurnWorker;
    use crate::tools::context::SharedTurnDiffTracker;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn start_turn_worker(
        session: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        router: ToolCapability,
        turn_diff_tracker: SharedTurnDiffTracker,
    ) -> Option<CodeModeTurnWorker> {
        session
            .services
            .code_mode_service
            .start_turn_worker(session, turn_context, router, turn_diff_tracker)
            .await
    }
}

pub(in crate::praxis::turn_loop_adapter::model_stream) mod tool_error_response {
    use praxis_protocol::models::FunctionCallOutputBody;
    use praxis_protocol::models::FunctionCallOutputPayload;
    use praxis_protocol::models::ResponseInputItem;
    use praxis_protocol::models::ResponseItem;

    use crate::turn_output_items::record_completed_response_item;
    use crate::turn_output_items::response_input_to_response_item;

    use super::PraxisModelStreamInput;

    pub(in crate::praxis::turn_loop_adapter::model_stream) async fn record_tool_error_response(
        input: &PraxisModelStreamInput,
        source_item: &ResponseItem,
        message: impl Into<String>,
    ) {
        let response = ResponseInputItem::FunctionCallOutput {
            call_id: String::new(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(message.into()),
                ..Default::default()
            },
        };

        record_completed_response_item(
            input.session.as_ref(),
            input.turn_context.as_ref(),
            source_item,
        )
        .await;

        if let Some(response_item) = response_input_to_response_item(&response) {
            input
                .session
                .record_conversation_items(
                    &input.turn_context,
                    std::slice::from_ref(&response_item),
                )
                .await;
        }
    }
}
