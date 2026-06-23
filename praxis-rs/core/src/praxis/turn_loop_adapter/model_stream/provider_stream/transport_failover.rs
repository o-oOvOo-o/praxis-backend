use crate::error::PraxisErr;

use super::super::PraxisModelStreamInput;

pub(super) async fn switch_to_http_transport(input: &PraxisModelStreamInput) -> bool {
    let mut runtime_state = input.runtime_state.lock().await;
    runtime_state
        .client_session_mut()
        .try_switch_http_transport(
            &input.turn_context.session_telemetry,
            &input.turn_context.model_info,
        )
}

pub(super) async fn warn_http_transport_failover(input: &PraxisModelStreamInput, err: &PraxisErr) {
    input
        .session
        .turn_event_emitter(&input.turn_context)
        .warning(format!(
            "Switching from WebSockets to HTTPS transport. {err:#}"
        ))
        .await;
}
