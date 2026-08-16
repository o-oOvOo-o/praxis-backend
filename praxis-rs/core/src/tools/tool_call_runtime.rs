use std::sync::Arc;
use std::time::Instant;

use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::Instrument;
use tracing::instrument;
use tracing::trace_span;

use crate::capabilities::ToolCapability;
use crate::error::PraxisErr;
use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::context::AbortedToolOutput;
use crate::tools::context::SharedTurnDiffTracker;
use crate::tools::context::ToolPayload;
use crate::tools::loop_guard::ToolLoopDecision;
use crate::tools::registry::AnyToolResult;
use crate::tools::registry::ToolPreparation;
use crate::tools::router::ToolCall;
use crate::tools::router::ToolCallSource;
use crate::tools::settlement::settle_with_cancellation;
use praxis_loop::tool::EffectJournal;
use praxis_protocol::models::ResponseInputItem;
use praxis_tools::ToolSpec;

#[derive(Clone)]
pub(crate) struct ToolCallRuntime {
    router: ToolCapability,
    code_mode_router: ToolCapability,
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    tracker: SharedTurnDiffTracker,
}

impl ToolCallRuntime {
    pub(crate) fn new(
        router: ToolCapability,
        code_mode_router: ToolCapability,
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        tracker: SharedTurnDiffTracker,
    ) -> Self {
        Self {
            router,
            code_mode_router,
            session,
            turn_context,
            tracker,
        }
    }

    pub(crate) fn find_spec(&self, tool_name: &str) -> Option<ToolSpec> {
        self.router.find_spec(tool_name)
    }

    pub(crate) async fn prepare_tool_call(
        &self,
        call: ToolCall,
    ) -> Result<ToolPreparation, FunctionCallError> {
        self.router
            .tool_preparation(
                Arc::clone(&self.session),
                Arc::clone(&self.turn_context),
                Arc::clone(&self.tracker),
                call,
            )
            .await
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn handle_tool_call(
        self,
        call: ToolCall,
        cancellation_token: CancellationToken,
    ) -> impl std::future::Future<Output = Result<ResponseInputItem, PraxisErr>> {
        self.handle_tool_call_observed(call, cancellation_token, EffectJournal::default())
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn handle_tool_call_observed(
        self,
        call: ToolCall,
        cancellation_token: CancellationToken,
        effect_journal: EffectJournal,
    ) -> impl std::future::Future<Output = Result<ResponseInputItem, PraxisErr>> {
        let error_call = call.clone();
        let future = self.handle_tool_call_with_source_and_effects(
            call,
            None,
            ToolCallSource::Direct,
            cancellation_token,
            effect_journal,
        );
        async move {
            match future.await {
                Ok(response) => Ok(response.into_response()),
                Err(FunctionCallError::Fatal(message)) => Err(PraxisErr::Fatal(message)),
                Err(other) => Ok(Self::failure_response(error_call, other)),
            }
        }
        .in_current_span()
    }

    pub(crate) fn handle_prepared_tool_call_observed(
        self,
        call: ToolCall,
        preparation: ToolPreparation,
        cancellation_token: CancellationToken,
        effect_journal: EffectJournal,
    ) -> impl std::future::Future<Output = Result<ResponseInputItem, PraxisErr>> {
        let error_call = call.clone();
        let future = self.handle_tool_call_with_source_and_effects(
            call,
            Some(preparation),
            ToolCallSource::Direct,
            cancellation_token,
            effect_journal,
        );
        async move {
            match future.await {
                Ok(response) => Ok(response.into_response()),
                Err(FunctionCallError::Fatal(message)) => Err(PraxisErr::Fatal(message)),
                Err(other) => Ok(Self::failure_response(error_call, other)),
            }
        }
        .in_current_span()
    }

    #[instrument(level = "trace", skip_all)]
    pub(crate) fn handle_tool_call_with_source(
        self,
        call: ToolCall,
        source: ToolCallSource,
        cancellation_token: CancellationToken,
    ) -> impl std::future::Future<Output = Result<AnyToolResult, FunctionCallError>> {
        self.handle_tool_call_with_source_and_effects(
            call,
            None,
            source,
            cancellation_token,
            EffectJournal::default(),
        )
    }

    fn handle_tool_call_with_source_and_effects(
        self,
        call: ToolCall,
        preparation: Option<ToolPreparation>,
        source: ToolCallSource,
        cancellation_token: CancellationToken,
        effect_journal: EffectJournal,
    ) -> impl std::future::Future<Output = Result<AnyToolResult, FunctionCallError>> {
        self.turn_context
            .tool_loop_guard
            .record_tool_call(call.tool_name.as_str());
        let wait_probe_decision = self
            .turn_context
            .tool_loop_guard
            .record_shell_wait_probe(call.tool_name.as_str(), &call.payload);
        let router = self.router.clone();
        let code_mode_router = self.code_mode_router.clone();
        let session = Arc::clone(&self.session);
        let turn = Arc::clone(&self.turn_context);
        let tracker = Arc::clone(&self.tracker);
        let started = Instant::now();

        let dispatch_span = trace_span!(
            "dispatch_tool_call_with_code_mode_result",
            otel.name = call.tool_name.as_str(),
            tool_name = call.tool_name.as_str(),
            call_id = call.call_id.as_str(),
            aborted = false,
        );

        let handle: AbortOnDropHandle<Result<AnyToolResult, FunctionCallError>> =
            AbortOnDropHandle::new(tokio::spawn(async move {
                if let ToolLoopDecision::Block { message } = wait_probe_decision {
                    return Err(FunctionCallError::RespondToModel(message));
                }
                let aborted_call = call.clone();
                let aborted_span = dispatch_span.clone();
                let dispatch = crate::tools::effects::scope_effect_journal(effect_journal, async {
                    match preparation {
                        Some(preparation) => {
                            router
                                .dispatch_prepared_tool_call_with_code_mode_result(
                                    session,
                                    turn,
                                    tracker,
                                    call.clone(),
                                    preparation,
                                    source,
                                    Some(code_mode_router.clone()),
                                )
                                .instrument(dispatch_span.clone())
                                .await
                        }
                        None => {
                            router
                                .dispatch_tool_call_with_code_mode_result(
                                    session,
                                    turn,
                                    tracker,
                                    call.clone(),
                                    source,
                                    Some(code_mode_router.clone()),
                                )
                                .instrument(dispatch_span.clone())
                                .await
                        }
                    }
                });
                settle_with_cancellation(dispatch, &cancellation_token, || {
                    let secs = started.elapsed().as_secs_f32().max(0.1);
                    aborted_span.record("aborted", true);
                    Ok(Self::aborted_response(&aborted_call, secs))
                })
                .await
            }));

        async move {
            handle.await.map_err(|err| {
                FunctionCallError::RespondToModel(format!("tool task failed to receive: {err:?}"))
            })?
        }
        .in_current_span()
    }
}

impl ToolCallRuntime {
    fn failure_response(call: ToolCall, err: FunctionCallError) -> ResponseInputItem {
        let message = err.to_string();
        match call.payload {
            ToolPayload::ToolSearch { .. } => ResponseInputItem::ToolSearchOutput {
                call_id: call.call_id,
                status: "completed".to_string(),
                execution: "client".to_string(),
                tools: Vec::new(),
            },
            ToolPayload::Custom { .. } => ResponseInputItem::CustomToolCallOutput {
                call_id: call.call_id,
                name: None,
                output: praxis_protocol::models::FunctionCallOutputPayload {
                    body: praxis_protocol::models::FunctionCallOutputBody::Text(message),
                    success: Some(false),
                },
            },
            _ => ResponseInputItem::FunctionCallOutput {
                call_id: call.call_id,
                output: praxis_protocol::models::FunctionCallOutputPayload {
                    body: praxis_protocol::models::FunctionCallOutputBody::Text(message),
                    success: Some(false),
                },
            },
        }
    }

    fn aborted_response(call: &ToolCall, secs: f32) -> AnyToolResult {
        AnyToolResult {
            call_id: call.call_id.clone(),
            payload: call.payload.clone(),
            result: Box::new(AbortedToolOutput {
                message: Self::abort_message(call, secs),
            }),
        }
    }

    fn abort_message(call: &ToolCall, secs: f32) -> String {
        match call.tool_name.as_str() {
            "shell" | "container.exec" | "local_shell" | "shell_command" | "unified_exec" => {
                format!("Wall time: {secs:.1} seconds\naborted by user")
            }
            _ => format!("aborted by user after {secs:.1}s"),
        }
    }
}
