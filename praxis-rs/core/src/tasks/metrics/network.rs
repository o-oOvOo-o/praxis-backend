use praxis_otel::SessionTelemetry;
use praxis_otel::metrics::names::TURN_NETWORK_PROXY_METRIC;
use tracing::warn;

use crate::praxis::Session;

pub(super) async fn network_proxy_active(session: &Session) -> bool {
    match session.services.network_proxy.as_ref() {
        Some(started_network_proxy) => match started_network_proxy.proxy().current_cfg().await {
            Ok(config) => config.network.enabled,
            Err(err) => {
                warn!("failed to read managed network proxy state for turn metrics: {err:#}");
                false
            }
        },
        None => false,
    }
}

pub(super) fn emit_turn_network_proxy_metric(
    session_telemetry: &SessionTelemetry,
    network_proxy_active: bool,
    tmp_mem: (&str, &str),
) {
    let active = if network_proxy_active {
        "true"
    } else {
        "false"
    };
    session_telemetry.counter(
        TURN_NETWORK_PROXY_METRIC,
        /*inc*/ 1,
        &[("active", active), tmp_mem],
    );
}
