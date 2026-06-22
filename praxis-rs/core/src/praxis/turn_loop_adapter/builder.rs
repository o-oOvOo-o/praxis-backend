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
            prompt_bridge::initial_prompt_items_from_session_history(&sess, &turn_context).await;

        PraxisTurnLoopBridge {
            ctx: context::build_context(sess.as_ref(), turn_context.as_ref(), initial_prompt_items),
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
