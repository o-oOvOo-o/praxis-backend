use praxis_loop::services::ModelRequest;

use super::super::super::round_input;
use super::super::super::round_input::PraxisRoundInput;
use super::super::PraxisModelStreamInput;

pub(super) async fn project_round_input(
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
