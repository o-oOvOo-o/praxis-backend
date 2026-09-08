use super::super::Session;
use super::super::TurnContext;
use super::model_round_state::PraxisModelRoundState;
use super::state::PraxisTurnBridgeState;
use super::steering_decision;
use super::tool_runtime_slot::ModelRoundToolsSlot;
use praxis_loop::services::SteeringDrain;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::client::ModelClientSession;
use crate::tools::context::SharedTurnDiffTracker;

mod event_sink {
    use async_trait::async_trait;
    use praxis_loop::model::TurnEvent;
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::services::EventSink;

    use super::PraxisTurnServices;
    use super::loop_event_sink_projection;

    #[async_trait]
    impl EventSink for PraxisTurnServices {
        async fn emit_event(&self, event: TurnEvent) -> LoopResult<()> {
            loop_event_sink_projection::emit_loop_event(
                self.session(),
                self.turn_context(),
                self.turn_diff_tracker().await,
                event,
            )
            .await;
            Ok(())
        }
    }
}
mod history {
    use async_trait::async_trait;
    use praxis_loop::model::TurnItem as LoopTurnItem;
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::services::HistorySink;

    use super::super::history_bridge;
    use super::PraxisTurnServices;

    #[async_trait]
    impl HistorySink for PraxisTurnServices {
        async fn persist_items(&self, items: &[LoopTurnItem]) -> LoopResult<()> {
            let response_items = history_bridge::loop_turn_items_to_response_items(items);
            for response_item in response_items {
                self.session
                    .record_response_item_and_emit_turn_item(&self.turn_context, response_item)
                    .await;
            }
            Ok(())
        }
    }
}
mod loop_event_sink_projection {
    use std::sync::Arc;

    use praxis_loop::model::TurnEvent;

    use crate::tools::context::SharedTurnDiffTracker;

    use super::super::super::Session;
    use super::super::super::TurnContext;
    use super::super::turn_event_emitter;

    pub(in crate::praxis::turn_loop_adapter) async fn emit_loop_event(
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        turn_diff_tracker: SharedTurnDiffTracker,
        event: TurnEvent,
    ) {
        match event {
            TurnEvent::TextDelta { item_id, text } => {
                turn_event_emitter::emit_text_delta(&session, &turn_context, item_id, text).await;
            }
            TurnEvent::ReasoningDelta {
                item_id,
                summary_index,
                content_index,
                text,
            } => {
                turn_event_emitter::emit_reasoning_delta(
                    &session,
                    &turn_context,
                    item_id,
                    summary_index,
                    content_index,
                    text,
                )
                .await;
            }
            TurnEvent::ToolStarted { .. } | TurnEvent::ToolProgress { .. } => {}
            TurnEvent::ToolExecutionCompleted(_)
            | TurnEvent::ToolFinished(_)
            | TurnEvent::TurnCompleted => {
                turn_event_emitter::emit_turn_diff_if_present(
                    &session,
                    &turn_context,
                    &turn_diff_tracker,
                )
                .await;
            }
            TurnEvent::TurnAborted(_) => {}
        }
    }
}
mod model_service {
    use std::sync::Arc;

    use async_trait::async_trait;
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::services::ModelEventStream;
    use praxis_loop::services::ModelRequest;
    use praxis_loop::services::ModelService;
    use tokio_util::sync::CancellationToken;

    use super::super::model_stream;
    use super::super::model_stream::PraxisModelStreamInput;
    use super::PraxisTurnServices;

    #[async_trait]
    impl ModelService for PraxisTurnServices {
        async fn stream_model(
            &self,
            request: ModelRequest,
            cancellation_token: CancellationToken,
        ) -> LoopResult<ModelEventStream> {
            model_stream::stream_model(
                PraxisModelStreamInput {
                    session: Arc::clone(&self.session),
                    turn_context: Arc::clone(&self.turn_context),
                    bridge_state: Arc::clone(&self.bridge_state),
                    runtime_state: Arc::clone(&self.runtime_state),
                    tool_runtime_slot: self.tool_runtime_slot.clone(),
                },
                request,
                cancellation_token,
            )
            .await
        }
    }
}
mod steering {
    use async_trait::async_trait;
    use praxis_loop::outcome::LoopResult;
    use praxis_loop::services::SteeringDrain;
    use praxis_loop::services::SteeringInbox;

    use super::PraxisTurnServices;

    #[async_trait]
    impl SteeringInbox for PraxisTurnServices {
        async fn drain_steering(&self) -> LoopResult<SteeringDrain> {
            Ok(self.process_pending_input_for_round().await)
        }

        async fn wait_for_steering(&self) -> LoopResult<()> {
            self.session.wait_for_pending_steer().await;
            Ok(())
        }
    }
}
mod tool_access {
    use std::sync::Arc;

    use praxis_loop::services::ToolAccess;
    use praxis_loop::tool::Tool;

    use super::super::tool_bridge;
    use super::PraxisTurnServices;

    impl ToolAccess for PraxisTurnServices {
        fn resolve_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
            let runtime = self.tool_runtime_slot.current()?;
            tool_bridge::resolve_tool_from_runtime(&runtime, name)
        }
    }
}

pub(in crate::praxis::turn_loop_adapter) struct PraxisTurnServices {
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    bridge_state: Arc<PraxisTurnBridgeState>,
    runtime_state: Arc<Mutex<PraxisModelRoundState>>,
    tool_runtime_slot: ModelRoundToolsSlot,
}

impl PraxisTurnServices {
    pub(in crate::praxis::turn_loop_adapter) fn new(
        sess: Arc<Session>,
        turn_context: Arc<TurnContext>,
        bridge_state: Arc<PraxisTurnBridgeState>,
        prewarmed_client_session: Option<ModelClientSession>,
    ) -> Self {
        let runtime_state = PraxisModelRoundState::new(
            sess.as_ref(),
            turn_context.as_ref(),
            prewarmed_client_session,
        );
        Self {
            session: sess,
            turn_context,
            bridge_state,
            runtime_state: Arc::new(Mutex::new(runtime_state)),
            tool_runtime_slot: ModelRoundToolsSlot::default(),
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn session(&self) -> Arc<Session> {
        Arc::clone(&self.session)
    }

    pub(in crate::praxis::turn_loop_adapter) fn turn_context(&self) -> Arc<TurnContext> {
        Arc::clone(&self.turn_context)
    }

    pub(in crate::praxis::turn_loop_adapter) async fn turn_diff_tracker(
        &self,
    ) -> SharedTurnDiffTracker {
        self.runtime_state.lock().await.turn_diff_tracker()
    }

    pub(in crate::praxis::turn_loop_adapter) async fn last_agent_message(&self) -> Option<String> {
        self.bridge_state.last_agent_message().await
    }

    pub(in crate::praxis::turn_loop_adapter) async fn process_pending_input_for_round(
        &self,
    ) -> SteeringDrain {
        steering_decision::process_pending_input_for_round(&self.session, &self.turn_context).await
    }
}
