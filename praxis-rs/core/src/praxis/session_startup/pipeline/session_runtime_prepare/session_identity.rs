mod input;
mod runtime;
mod shell_phase;
mod telemetry_phase;

mod thread_name;

pub(super) use input::SessionIdentityRuntimeInput;
pub(super) use runtime::SessionIdentityRuntime;

pub(super) async fn prepare(
    input: SessionIdentityRuntimeInput<'_>,
) -> anyhow::Result<SessionIdentityRuntime> {
    let telemetry_phase::SessionTelemetryRuntime {
        session_telemetry,
        network_proxy_audit_metadata,
    } = telemetry_phase::build(telemetry_phase::TelemetryPhaseInput {
        conversation_id: input.conversation_id,
        config: input.config,
        auth_manager: input.auth_manager,
        auth: input.auth,
        session_configuration: input.session_configuration,
        mcp_servers: input.mcp_servers,
    });

    let shell_phase::SessionShellRuntime {
        shell: default_shell,
        snapshot_tx: shell_snapshot_tx,
    } = shell_phase::build(shell_phase::ShellPhaseInput {
        conversation_id: input.conversation_id,
        config: input.config,
        session_configuration: input.session_configuration,
        session_telemetry: &session_telemetry,
    })?;

    thread_name::resolve_and_assign(thread_name::ThreadNameInput {
        conversation_id: input.conversation_id,
        forked_from_id: input.forked_from_id,
        initial_history: input.initial_history,
        state_db_ctx: input.state_db_ctx,
        config: input.config,
        session_configuration: input.session_configuration,
    })
    .await;

    Ok(SessionIdentityRuntime {
        session_telemetry,
        network_proxy_audit_metadata,
        default_shell,
        shell_snapshot_tx,
    })
}
