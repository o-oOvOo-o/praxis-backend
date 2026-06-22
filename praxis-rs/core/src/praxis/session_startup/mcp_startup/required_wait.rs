use std::sync::Arc;

use tracing::Instrument;
use tracing::info_span;

use crate::praxis::Session;

pub(super) async fn wait(
    session: &Arc<Session>,
    required_servers: &[String],
    required_count: usize,
) -> anyhow::Result<()> {
    if required_servers.is_empty() {
        return Ok(());
    }

    let failures = session
        .services
        .mcp_connection_manager
        .read()
        .await
        .required_startup_failures(required_servers)
        .instrument(info_span!(
            "session_init.required_mcp_wait",
            otel.name = "session_init.required_mcp_wait",
            session_init.required_mcp_server_count = required_count,
        ))
        .await;
    if failures.is_empty() {
        return Ok(());
    }

    let details = failures
        .iter()
        .map(|failure| format!("{}: {}", failure.server, failure.error))
        .collect::<Vec<_>>()
        .join("; ");
    Err(anyhow::anyhow!(
        "required MCP servers failed to initialize: {details}"
    ))
}
