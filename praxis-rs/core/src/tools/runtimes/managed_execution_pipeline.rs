use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;

use praxis_network_proxy::NetworkProxy;
use praxis_protocol::models::PermissionProfile;
use praxis_shell_command::powershell::prefix_powershell_script_with_utf8;

use crate::agent_os::ManagedCommandSpan;
use crate::agent_os::process_runtime_kind;
use crate::agent_os::process_runtime_owner;
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecExpiration;
use crate::exec::ExecToolCallOutput;
use crate::exec::SpawnObserver;
use crate::exec::StdoutStream;
use crate::sandboxing::ExecOptions;
use crate::sandboxing::ExecRequest;
use crate::sandboxing::execute_env_with_spawn_observer;
use crate::shell::ShellType;
use crate::tools::runtimes::agent_os_command::finish_agent_os_span_with_output;
use crate::tools::runtimes::agent_os_command::finish_failed_agent_os_span;
use crate::tools::runtimes::agent_os_command::start_agent_os_command_span_with_runtime_route;
use crate::tools::runtimes::build_sandbox_command;
use crate::tools::runtimes::maybe_wrap_shell_lc_with_snapshot;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;

/// Shared AgentOS preflight for command-like side effects.
pub(crate) async fn preflight_command_intent(
    ctx: &ToolCtx,
    command: &[String],
    cwd: &Path,
) -> Result<(), ToolError> {
    ctx.session
        .services
        .agent_os
        .preflight_command_intent(ctx.session.conversation_id, command, cwd)
        .await
        .map(|_| ())
        .map_err(|err| ToolError::Rejected(err.to_string()))
}

/// Identity of the concrete backend that will execute a command.
///
/// This is deliberately separate from the command intent. Intent answers
/// "what resource semantics does this action have?"; a route answers "which
/// runtime backend owns the process if this action spawns one?".
#[derive(Clone, Copy, Debug)]
pub(crate) struct RuntimeExecutionRoute<'a> {
    pub(crate) label: &'static str,
    pub(crate) runtime_kind: &'static str,
    pub(crate) runtime_owner_id: Option<&'a str>,
    pub(crate) process_id: Option<i32>,
}

impl<'a> RuntimeExecutionRoute<'a> {
    pub(crate) const fn new(
        label: &'static str,
        runtime_kind: &'static str,
        runtime_owner_id: Option<&'a str>,
        process_id: Option<i32>,
    ) -> Self {
        Self {
            label,
            runtime_kind,
            runtime_owner_id,
            process_id,
        }
    }

    pub(crate) const fn shell() -> Self {
        Self::new(
            "shell",
            process_runtime_kind::SHELL,
            Some(process_runtime_owner::SHELL),
            None,
        )
    }

    pub(crate) const fn zsh_fork() -> Self {
        Self::new(
            "zsh-fork",
            process_runtime_kind::ZSH_FORK,
            Some(process_runtime_owner::ZSH_FORK),
            None,
        )
    }

    pub(crate) const fn apply_patch() -> Self {
        Self::new("apply_patch", process_runtime_kind::APPLY_PATCH, None, None)
    }

    pub(crate) const fn unified_exec(runtime_owner_id: &'a str, process_id: i32) -> Self {
        Self::new(
            "unified_exec",
            process_runtime_kind::UNIFIED_EXEC,
            Some(runtime_owner_id),
            Some(process_id),
        )
    }

    fn attaches_spawned_process(self) -> bool {
        self.process_id.is_none()
            && matches!(
                self.runtime_kind,
                process_runtime_kind::SHELL | process_runtime_kind::ZSH_FORK
            )
    }
}

/// Minimal backend descriptor used by the pipeline. Concrete runtimes should
/// expose a route instead of open-coding AgentOS labels, runtime kinds, owner
/// ids, and process ids at each call site.
pub(crate) trait RuntimeBackend {
    fn execution_route(&self) -> RuntimeExecutionRoute<'_>;
}

impl<'a> RuntimeBackend for RuntimeExecutionRoute<'a> {
    fn execution_route(&self) -> RuntimeExecutionRoute<'_> {
        *self
    }
}

/// Start an AgentOS span from a backend descriptor. Prefer this over passing
/// raw labels/kinds/owners at call sites; direct route passing is kept for
/// already-prepared routes and compatibility with older runtime adapters.
pub(crate) async fn start_agent_os_span_for_backend(
    ctx: &ToolCtx,
    raw_command: &[String],
    cwd: &Path,
    backend: &impl RuntimeBackend,
) -> Result<ManagedCommandSpan, ToolError> {
    start_agent_os_span_for_route(ctx, raw_command, cwd, backend.execution_route()).await
}

/// Sandbox preparation knobs shared by shell-like runtimes.
pub(crate) struct SandboxCommandSpec<'a> {
    pub(crate) command: &'a [String],
    pub(crate) cwd: &'a Path,
    pub(crate) env: &'a HashMap<String, String>,
    pub(crate) additional_permissions: Option<PermissionProfile>,
    pub(crate) network: Option<&'a NetworkProxy>,
    pub(crate) expiration: ExecExpiration,
    pub(crate) capture_policy: ExecCapturePolicy,
}

/// Higher-level command preparation spec used by shell-like runtimes.
///
/// The raw command is the user/model action recorded by AgentOS; the prepared
/// command is the session-shell adjusted argv that actually enters the sandbox
/// transform. Keeping both in one spec prevents shell/unified/apply_patch from
/// each open-coding slightly different command preparation behavior.
pub(crate) struct ShellSandboxSpec<'a> {
    pub(crate) raw_command: &'a [String],
    pub(crate) cwd: &'a Path,
    pub(crate) env: &'a HashMap<String, String>,
    pub(crate) explicit_env_overrides: &'a HashMap<String, String>,
    pub(crate) additional_permissions: Option<PermissionProfile>,
    pub(crate) network: Option<&'a NetworkProxy>,
    pub(crate) expiration: ExecExpiration,
    pub(crate) capture_policy: ExecCapturePolicy,
    /// Some runtime backends need managed proxy variables to be present before
    /// they hand the request to a child process manager. The sandbox transform
    /// still receives the NetworkProxy so policy enforcement stays centralized.
    pub(crate) apply_managed_network_env: bool,
}

pub(crate) struct PreparedSandboxedCommand {
    pub(crate) exec_request: ExecRequest,
}

/// Apply shell snapshot and PowerShell UTF-8 handling exactly once for all
/// shell-like command runtimes.
pub(crate) fn prepare_session_shell_command(
    ctx: &ToolCtx,
    command: &[String],
    cwd: &Path,
    explicit_env_overrides: &HashMap<String, String>,
) -> Vec<String> {
    let session_shell = ctx.session.user_shell();
    let command = maybe_wrap_shell_lc_with_snapshot(
        command,
        session_shell.as_ref(),
        cwd,
        explicit_env_overrides,
    );
    if matches!(session_shell.shell_type, ShellType::PowerShell) {
        prefix_powershell_script_with_utf8(&command)
    } else {
        command
    }
}

/// Convert a session-shell adjusted argv into a sandboxed ExecRequest.
pub(crate) fn prepare_sandboxed_exec_request(
    attempt: &SandboxAttempt<'_>,
    spec: SandboxCommandSpec<'_>,
) -> Result<ExecRequest, ToolError> {
    let command = build_sandbox_command(
        spec.command,
        spec.cwd,
        spec.env,
        spec.additional_permissions,
    )?;
    let options = ExecOptions {
        expiration: spec.expiration,
        capture_policy: spec.capture_policy,
    };
    attempt
        .env_for(command, options, spec.network)
        .map_err(|err| ToolError::Praxis(err.into()))
}

/// Prepare both the session-shell adjusted argv and the sandbox ExecRequest.
///
/// This is the canonical command preparation step for shell-like runtimes. It
/// keeps PowerShell UTF-8 handling, shell snapshot wrapping, network env
/// injection, sandbox transforms, and capture/timeout policy in one place.
pub(crate) fn prepare_shell_sandboxed_command(
    ctx: &ToolCtx,
    attempt: &SandboxAttempt<'_>,
    spec: ShellSandboxSpec<'_>,
) -> Result<PreparedSandboxedCommand, ToolError> {
    let command =
        prepare_session_shell_command(ctx, spec.raw_command, spec.cwd, spec.explicit_env_overrides);
    let mut env = spec.env.clone();
    if spec.apply_managed_network_env {
        if let Some(network) = spec.network {
            network.apply_to_env(&mut env);
        }
    }
    let exec_request = prepare_sandboxed_exec_request(
        attempt,
        SandboxCommandSpec {
            command: &command,
            cwd: spec.cwd,
            env: &env,
            additional_permissions: spec.additional_permissions,
            network: spec.network,
            expiration: spec.expiration,
            capture_policy: spec.capture_policy,
        },
    )?;
    Ok(PreparedSandboxedCommand { exec_request })
}

/// Record a known dirty-file set against a managed command span and finish the
/// span as failed if policy validation rejects it. This is the single helper
/// used by direct apply_patch, intercepted apply_patch, and future managed file
/// APIs so dirty-file scope violations are accounted consistently.
pub(crate) async fn record_known_dirty_files_or_finish(
    command_span: &ManagedCommandSpan,
    dirty_files: Vec<PathBuf>,
) -> Result<(), ToolError> {
    if dirty_files.is_empty() {
        return Ok(());
    }
    if let Err(err) = command_span.record_dirty_files(dirty_files).await {
        let error_text = err.to_string();
        let _ = command_span.finish_failure(error_text.as_bytes()).await;
        return Err(ToolError::Rejected(error_text));
    }
    Ok(())
}

/// Finish a non-exec managed command with success bytes. Used by direct
/// in-process actions such as apply_patch that are still side effects but do
/// not go through execute_env.

/// Record known dirty files and then execute a pre-started one-shot command.
///
/// This is the post-preflight hook for managed write operations. It ensures
/// dirty-file policy failures are represented as AgentOS command failures and
/// that direct/delegated apply_patch paths cannot drift apart.
pub(crate) async fn run_prestarted_one_shot_with_known_dirty_files(
    command_span: &ManagedCommandSpan,
    route: RuntimeExecutionRoute<'_>,
    dirty_files: Vec<PathBuf>,
    exec_request: ExecRequest,
    stdout_stream: Option<StdoutStream>,
) -> Result<ExecToolCallOutput, ToolError> {
    record_known_dirty_files_or_finish(command_span, dirty_files).await?;
    run_prestarted_one_shot_exec_with_agent_os(command_span, route, exec_request, stdout_stream)
        .await
}

pub(crate) async fn finish_agent_os_span_success_bytes(
    command_span: &ManagedCommandSpan,
    runtime_label: &str,
    raw_output: &[u8],
) {
    if let Err(err) = command_span.finish_success(raw_output).await {
        tracing::warn!("failed to finish AgentOS {runtime_label} command span: {err}");
    }
}

/// Start an AgentOS command span for the concrete backend route.
pub(crate) async fn start_agent_os_span_for_route(
    ctx: &ToolCtx,
    raw_command: &[String],
    cwd: &Path,
    route: RuntimeExecutionRoute<'_>,
) -> Result<ManagedCommandSpan, ToolError> {
    start_agent_os_command_span_with_runtime_route(
        ctx,
        raw_command,
        cwd,
        route.process_id,
        route.runtime_kind,
        route.runtime_owner_id,
    )
    .await
}

/// Start a long-lived/process-backed runtime with AgentOS accounting.
///
/// This is the process-oriented sibling of `run_one_shot_exec_with_agent_os`: it
/// acquires the ticket/leases before spawn and normalizes failed-open cleanup,
/// while leaving backend-specific process construction to the caller.
pub(crate) async fn run_process_open_with_agent_os<T, F, Fut>(
    ctx: &ToolCtx,
    raw_command: &[String],
    cwd: &Path,
    route: RuntimeExecutionRoute<'_>,
    open: F,
) -> Result<T, ToolError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, ToolError>>,
{
    let command_span = start_agent_os_span_for_route(ctx, raw_command, cwd, route).await?;
    let result = open().await;
    if let Err(err) = &result {
        finish_failed_agent_os_span(&command_span, route.label, err).await;
    }
    result
}

/// Execute a one-shot command using a pre-started AgentOS span.
///
/// This is used by runtimes such as apply_patch that need to attach dirty-file
/// metadata after acquiring the ticket but before spawning.
pub(crate) async fn run_prestarted_one_shot_exec_with_agent_os(
    command_span: &ManagedCommandSpan,
    route: RuntimeExecutionRoute<'_>,
    mut exec_request: ExecRequest,
    stdout_stream: Option<StdoutStream>,
) -> Result<ExecToolCallOutput, ToolError> {
    exec_request.raw_output_spool = true;
    let spawn_observer = route
        .attaches_spawned_process()
        .then(|| process_registry_spawn_observer(command_span, route.label));
    let mut result = execute_env_with_spawn_observer(exec_request, stdout_stream, spawn_observer)
        .await
        .map_err(ToolError::Praxis);
    match &mut result {
        Ok(output) => finish_agent_os_span_with_output(command_span, output, route.label).await,
        Err(err) => finish_failed_agent_os_span(command_span, route.label, err).await,
    }
    result
}

fn process_registry_spawn_observer(
    command_span: &ManagedCommandSpan,
    runtime_label: &'static str,
) -> SpawnObserver {
    let command_span = command_span.clone();
    Box::new(move |process_id| {
        let future: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> = Box::pin(
            async move {
                let Some(process_id) = process_id.and_then(|value| i32::try_from(value).ok())
                else {
                    tracing::warn!(
                        "AgentOS {runtime_label} process registry could not attach missing or oversized pid"
                    );
                    return;
                };
                if let Err(err) = command_span.attach_process(process_id).await {
                    tracing::warn!(
                        "failed to attach AgentOS {runtime_label} process id {process_id}: {err}"
                    );
                }
            },
        );
        future
    })
}

/// Execute a one-shot command through a concrete backend route while keeping
/// AgentOS ticket, lease, artifact, and failure accounting identical for all
/// one-shot command runtimes.
pub(crate) async fn run_one_shot_exec_with_agent_os(
    ctx: &ToolCtx,
    raw_command: &[String],
    cwd: &Path,
    route: RuntimeExecutionRoute<'_>,
    exec_request: ExecRequest,
    stdout_stream: Option<StdoutStream>,
) -> Result<ExecToolCallOutput, ToolError> {
    let command_span = start_agent_os_span_for_route(ctx, raw_command, cwd, route).await?;
    run_prestarted_one_shot_exec_with_agent_os(&command_span, route, exec_request, stdout_stream)
        .await
}
