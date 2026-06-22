use crate::error::PraxisErr;

use super::super::PraxisModelStreamInput;

pub(super) async fn switch_to_fallback_transport(input: &PraxisModelStreamInput) -> bool {
    let mut runtime_state = input.runtime_state.lock().await;
    runtime_state
        .client_session_mut()
        .try_switch_fallback_transport(
            &input.turn_context.session_telemetry,
            &input.turn_context.model_info,
        )
}

pub(super) async fn warn_fallback_transport(input: &PraxisModelStreamInput, err: &PraxisErr) {
    input
        .session
        .turn_event_emitter(&input.turn_context)
        .warning(format!(
            "Falling back from WebSockets to HTTPS transport. {err:#}"
        ))
        .await;
}
