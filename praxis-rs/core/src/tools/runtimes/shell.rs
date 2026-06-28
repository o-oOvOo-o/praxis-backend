/*
Runtime: shell

Executes shell requests under the orchestrator: asks for approval when needed,
builds sandbox transform inputs, and runs them under the current SandboxAttempt.
*/
#[cfg(unix)]
pub(crate) mod unix_escalation;
pub(crate) mod zsh_fork_backend;

use crate::agent_os::AgentOsProcessCleaner;
use crate::agent_os::process_runtime_kind;
use crate::agent_os::process_runtime_owner;
use crate::command_canonicalization::canonicalize_command_for_approval;
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecToolCallOutput;
use crate::sandboxing::SandboxPermissions;
use crate::tools::network_approval::NetworkApprovalMode;
use crate::tools::network_approval::NetworkApprovalSpec;
use crate::tools::runtimes::agent_os_command::finish_abandoned_agent_os_span;
use crate::tools::runtimes::agent_os_command::finish_agent_os_span_with_output;
use crate::tools::runtimes::agent_os_command::finish_failed_agent_os_span;
use crate::tools::runtimes::managed_execution_pipeline::RuntimeBackend;
use crate::tools::runtimes::managed_execution_pipeline::RuntimeExecutionRoute;
use crate::tools::runtimes::managed_execution_pipeline::ShellSandboxSpec;
use crate::tools::runtimes::managed_execution_pipeline::preflight_command_intent;
use crate::tools::runtimes::managed_execution_pipeline::prepare_session_shell_command;
use crate::tools::runtimes::managed_execution_pipeline::prepare_shell_sandboxed_command;
use crate::tools::runtimes::managed_execution_pipeline::run_one_shot_exec_with_agent_os;
use crate::tools::runtimes::managed_execution_pipeline::start_agent_os_span_for_backend;
use crate::tools::runtimes::runtime_approval::RuntimeApprovalKind;
use crate::tools::runtimes::runtime_approval::RuntimeApprovalPlan;
use crate::tools::runtimes::runtime_approval::start_runtime_approval_async;
use crate::tools::sandboxing::Approvable;
use crate::tools::sandboxing::ApprovalCtx;
use crate::tools::sandboxing::ExecApprovalRequirement;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::SandboxOverride;
use crate::tools::sandboxing::Sandboxable;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use crate::tools::sandboxing::ToolRuntime;
use crate::tools::sandboxing::sandbox_override_for_first_attempt;
use futures::future::BoxFuture;
use praxis_network_proxy::NetworkProxy;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::protocol::ReviewDecision;
use praxis_sandboxing::SandboxablePreference;
use praxis_system_plugin_approval_control::tool_safety::SafetyToolKind;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ShellRequest {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub timeout_ms: Option<u64>,
    pub env: HashMap<String, String>,
    pub explicit_env_overrides: HashMap<String, String>,
    pub network: Option<NetworkProxy>,
    pub sandbox_permissions: SandboxPermissions,
    pub additional_permissions: Option<PermissionProfile>,
    #[cfg(unix)]
    pub additional_permissions_preapproved: bool,
    pub justification: Option<String>,
    pub exec_approval_requirement: ExecApprovalRequirement,
}

/// Selects `ShellRuntime` behavior for different callers.
///
/// Note: `Generic` is not the same as `ShellCommandClassic`.
/// `Generic` means "no `shell_command`-specific backend behavior" (used by the
/// generic `shell` tool path). The `ShellCommand*` variants are only for the
/// `shell_command` tool family.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ShellRuntimeBackend {
    /// Tool-agnostic/default runtime path.
    ///
    /// Uses the normal `ShellRuntime` execution flow without enabling any
    /// `shell_command`-specific backend selection.
    #[default]
    Generic,
    /// Legacy backend for the `shell_command` tool.
    ///
    /// Keeps `shell_command` on the standard shell runtime flow without the
    /// zsh-fork shell-escalation adapter.
    ShellCommandClassic,
    /// zsh-fork backend for the `shell_command` tool.
    ///
    /// On Unix, attempts to run via the zsh-fork + `praxis-shell-escalation`
    /// adapter, with fallback to the standard shell runtime flow if
    /// prerequisites are not met.
    ShellCommandZshFork,
}

impl RuntimeBackend for ShellRuntimeBackend {
    fn execution_route(&self) -> RuntimeExecutionRoute<'_> {
        match self {
            Self::ShellCommandZshFork => RuntimeExecutionRoute::zsh_fork(),
            Self::Generic | Self::ShellCommandClassic => RuntimeExecutionRoute::shell(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellHostProcessCleaner {
    runtime_kind: &'static str,
    runtime_owner_id: &'static str,
}

impl ShellHostProcessCleaner {
    pub(crate) fn shell() -> Self {
        Self {
            runtime_kind: process_runtime_kind::SHELL,
            runtime_owner_id: process_runtime_owner::SHELL,
        }
    }

    pub(crate) fn zsh_fork() -> Self {
        Self {
            runtime_kind: process_runtime_kind::ZSH_FORK,
            runtime_owner_id: process_runtime_owner::ZSH_FORK,
        }
    }
}

#[async_trait::async_trait]
impl AgentOsProcessCleaner for ShellHostProcessCleaner {
    fn runtime_kind(&self) -> &'static str {
        self.runtime_kind
    }

    fn runtime_owner_id(&self) -> String {
        self.runtime_owner_id.to_string()
    }

    async fn cleanup_agent_os_process(&self, process_id: i32) -> bool {
        kill_host_process_tree(process_id).await
    }
}

async fn kill_host_process_tree(process_id: i32) -> bool {
    let Ok(process_id) = u32::try_from(process_id) else {
        return false;
    };
    kill_host_process_tree_by_pid(process_id).await
}

#[cfg(unix)]
async fn kill_host_process_tree_by_pid(process_id: u32) -> bool {
    match praxis_utils_pty::process_group::kill_process_group_by_pid(process_id) {
        Ok(()) => true,
        Err(err) => {
            tracing::warn!("failed to kill shell process group for pid {process_id}: {err}");
            false
        }
    }
}

#[cfg(windows)]
async fn kill_host_process_tree_by_pid(process_id: u32) -> bool {
    let process_id = process_id.to_string();
    match tokio::process::Command::new("taskkill")
        .args(["/PID", process_id.as_str(), "/T", "/F"])
        .status()
        .await
    {
        Ok(status) => status.success(),
        Err(err) => {
            tracing::warn!("failed to run taskkill for shell pid {process_id}: {err}");
            false
        }
    }
}

#[cfg(not(any(unix, windows)))]
async fn kill_host_process_tree_by_pid(_process_id: u32) -> bool {
    false
}

#[derive(Default)]
pub struct ShellRuntime {
    backend: ShellRuntimeBackend,
}

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct ApprovalKey {
    command: Vec<String>,
    cwd: PathBuf,
    sandbox_permissions: SandboxPermissions,
    additional_permissions: Option<PermissionProfile>,
}

impl ShellRuntime {
    pub fn new() -> Self {
        Self {
            backend: ShellRuntimeBackend::Generic,
        }
    }

    pub(crate) fn for_shell_command(backend: ShellRuntimeBackend) -> Self {
        Self { backend }
    }

    fn stdout_stream(ctx: &ToolCtx) -> Option<crate::exec::StdoutStream> {
        Some(crate::exec::StdoutStream {
            sub_id: ctx.turn.sub_id.clone(),
            call_id: ctx.call_id.clone(),
            tx_event: ctx.session.get_tx_event(),
        })
    }
}

impl Sandboxable for ShellRuntime {
    fn tool_kind(&self) -> SafetyToolKind {
        SafetyToolKind::Exec
    }

    fn sandbox_preference(&self) -> SandboxablePreference {
        SandboxablePreference::Auto
    }
    fn escalate_on_failure(&self) -> bool {
        true
    }
}

impl Approvable<ShellRequest> for ShellRuntime {
    type ApprovalKey = ApprovalKey;

    fn approval_keys(&self, req: &ShellRequest) -> Vec<Self::ApprovalKey> {
        vec![ApprovalKey {
            command: canonicalize_command_for_approval(&req.command),
            cwd: req.cwd.clone(),
            sandbox_permissions: req.sandbox_permissions,
            additional_permissions: req.additional_permissions.clone(),
        }]
    }

    fn start_approval_async<'a>(
        &'a mut self,
        req: &'a ShellRequest,
        ctx: ApprovalCtx<'a>,
    ) -> BoxFuture<'a, ReviewDecision> {
        start_runtime_approval_async(
            self.approval_keys(req),
            RuntimeApprovalPlan {
                tool_name: "shell",
                kind: RuntimeApprovalKind::Shell,
                command: req.command.clone(),
                cwd: req.cwd.clone(),
                sandbox_permissions: req.sandbox_permissions,
                additional_permissions: req.additional_permissions.clone(),
                justification: req.justification.clone(),
                exec_approval_requirement: req.exec_approval_requirement.clone(),
            },
            ctx,
        )
    }

    fn exec_approval_requirement(&self, req: &ShellRequest) -> Option<ExecApprovalRequirement> {
        Some(req.exec_approval_requirement.clone())
    }

    fn sandbox_mode_for_first_attempt(&self, req: &ShellRequest) -> SandboxOverride {
        sandbox_override_for_first_attempt(req.sandbox_permissions, &req.exec_approval_requirement)
    }
}

impl ToolRuntime<ShellRequest, ExecToolCallOutput> for ShellRuntime {
    async fn preflight(&mut self, req: &ShellRequest, ctx: &ToolCtx) -> Result<(), ToolError> {
        preflight_command_intent(ctx, &req.command, &req.cwd).await
    }

    fn network_approval_spec(
        &self,
        req: &ShellRequest,
        _ctx: &ToolCtx,
    ) -> Option<NetworkApprovalSpec> {
        req.network.as_ref()?;
        Some(NetworkApprovalSpec {
            network: req.network.clone(),
            mode: NetworkApprovalMode::Immediate,
        })
    }

    async fn run(
        &mut self,
        req: &ShellRequest,
        attempt: &SandboxAttempt<'_>,
        ctx: &ToolCtx,
    ) -> Result<ExecToolCallOutput, ToolError> {
        if self.backend == ShellRuntimeBackend::ShellCommandZshFork {
            if zsh_fork_backend::can_run_shell_command(ctx) {
                let command = prepare_session_shell_command(
                    ctx,
                    &req.command,
                    &req.cwd,
                    &req.explicit_env_overrides,
                );
                let command_span = start_agent_os_span_for_backend(
                    ctx,
                    &req.command,
                    &req.cwd,
                    &ShellRuntimeBackend::ShellCommandZshFork,
                )
                .await?;
                match zsh_fork_backend::maybe_run_shell_command(
                    req,
                    attempt,
                    ctx,
                    &command,
                    &command_span,
                )
                .await
                {
                    Ok(Some(mut out)) => {
                        finish_agent_os_span_with_output(&command_span, &mut out, "zsh-fork").await;
                        return Ok(out);
                    }
                    Ok(None) => {
                        finish_abandoned_agent_os_span(
                            &command_span,
                            "zsh-fork",
                            b"zsh-fork declined after capability precheck; falling back",
                        )
                        .await;
                        preflight_command_intent(ctx, &req.command, &req.cwd).await?;
                        tracing::warn!(
                            "ZshFork backend specified, but declined after precheck, falling back to normal execution",
                        );
                    }
                    Err(err) => {
                        finish_failed_agent_os_span(&command_span, "zsh-fork", &err).await;
                        return Err(err);
                    }
                }
            } else {
                tracing::warn!(
                    "ZshFork backend specified, but side-effect-free capability checks failed, falling back to normal execution",
                );
            }
        }

        let prepared = prepare_shell_sandboxed_command(
            ctx,
            attempt,
            ShellSandboxSpec {
                raw_command: &req.command,
                cwd: &req.cwd,
                env: &req.env,
                explicit_env_overrides: &req.explicit_env_overrides,
                additional_permissions: req.additional_permissions.clone(),
                network: req.network.as_ref(),
                expiration: req.timeout_ms.into(),
                capture_policy: ExecCapturePolicy::ShellTool,
                apply_managed_network_env: false,
            },
        )?;
        run_one_shot_exec_with_agent_os(
            ctx,
            &req.command,
            &req.cwd,
            ShellRuntimeBackend::Generic.execution_route(),
            prepared.exec_request,
            Self::stdout_stream(ctx),
        )
        .await
    }
}
