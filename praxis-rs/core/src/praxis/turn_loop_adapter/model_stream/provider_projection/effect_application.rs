use super::effect::ProviderEffect;
use super::effect::apply_provider_effect;

use super::super::PraxisModelStreamInput;

pub(super) async fn apply_core_effect(input: &PraxisModelStreamInput, effect: ProviderEffect) {
    let mut runtime_state = input.runtime_state.lock().await;
    apply_provider_effect(
        &input.session,
        &input.turn_context,
        effect,
        runtime_state.server_model_warning_emitted_for_turn_mut(),
    )
    .await;
}
