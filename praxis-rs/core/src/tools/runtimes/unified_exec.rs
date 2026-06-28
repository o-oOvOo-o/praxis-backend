/*
Runtime: unified exec

Handles approval + sandbox orchestration for unified exec requests, delegating to
the process manager to spawn PTYs once an ExecRequest is prepared.
*/
use crate::command_canonicalization::canonicalize_command_for_approval;
use crate::error::PraxisErr;
use crate::error::SandboxErr;
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecExpiration;
use crate::sandboxing::SandboxPermissions;
use crate::tools::network_approval::NetworkApprovalMode;
use crate::tools::network_approval::NetworkApprovalSpec;
use crate::tools::runtimes::managed_execution_pipeline::RuntimeExecutionRoute;
use crate::tools::runtimes::managed_execution_pipeline::ShellSandboxSpec;
use crate::tools::runtimes::managed_execution_pipeline::preflight_command_intent;
use crate::tools::runtimes::managed_execution_pipeline::prepare_shell_sandboxed_command;
use crate::tools::runtimes::managed_execution_pipeline::run_process_open_with_agent_os;
use crate::tools::runtimes::runtime_approval::RuntimeApprovalKind;
use crate::tools::runtimes::runtime_approval::RuntimeApprovalPlan;
use crate::tools::runtimes::runtime_approval::start_runtime_approval_async;
use crate::tools::runtimes::shell::zsh_fork_backend;
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
use crate::unified_exec::NoopSpawnLifecycle;
use crate::unified_exec::UnifiedExecError;
use crate::unified_exec::UnifiedExecProcess;
use crate::unified_exec::UnifiedExecProcessManager;
use futures::future::BoxFuture;
use praxis_network_proxy::NetworkProxy;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::protocol::ReviewDecision;
use praxis_sandboxing::SandboxablePreference;
use praxis_system_plugin_approval_control::tool_safety::SafetyToolKind;
use praxis_tools::UnifiedExecShellMode;
use std::collections::HashMap;
use std::path::PathBuf;

/// Request payload used by the unified-exec runtime after approvals and
/// sandbox preferences have been resolved for the current turn.
#[derive(Clone, Debug)]
pub struct UnifiedExecRequest {
    pub command: Vec<String>,
    pub process_id: i32,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub explicit_env_overrides: HashMap<String, String>,
    pub network: Option<NetworkProxy>,
    pub tty: bool,
    pub sandbox_permissions: SandboxPermissions,
    pub additional_permissions: Option<PermissionProfile>,
    #[cfg(unix)]
    pub additional_permissions_preapproved: bool,
    pub justification: Option<String>,
    pub exec_approval_requirement: ExecApprovalRequirement,
}

/// Cache key for approval decisions that can be reused across equivalent
/// unified-exec launches.
#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct UnifiedExecApprovalKey {
    pub command: Vec<String>,
    pub cwd: PathBuf,
    pub tty: bool,
    pub sandbox_permissions: SandboxPermissions,
    pub additional_permissions: Option<PermissionProfile>,
}

/// Runtime adapter that keeps policy and sandbox orchestration on the
/// unified-exec side while delegating process startup to the manager.
pub struct UnifiedExecRuntime<'a> {
    manager: &'a UnifiedExecProcessManager,
    shell_mode: UnifiedExecShellMode,
}

impl<'a> UnifiedExecRuntime<'a> {
    /// Creates a runtime bound to the shared unified-exec process manager.
    pub fn new(manager: &'a UnifiedExecProcessManager, shell_mode: UnifiedExecShellMode) -> Self {
        Self {
            manager,
            shell_mode,
        }
    }
}

impl Sandboxable for UnifiedExecRuntime<'_> {
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

impl Approvable<UnifiedExecRequest> for UnifiedExecRuntime<'_> {
    type ApprovalKey = UnifiedExecApprovalKey;

    fn approval_keys(&self, req: &UnifiedExecRequest) -> Vec<Self::ApprovalKey> {
        vec![UnifiedExecApprovalKey {
            command: canonicalize_command_for_approval(&req.command),
            cwd: req.cwd.clone(),
            tty: req.tty,
            sandbox_permissions: req.sandbox_permissions,
            additional_permissions: req.additional_permissions.clone(),
        }]
    }

    fn start_approval_async<'b>(
        &'b mut self,
        req: &'b UnifiedExecRequest,
        ctx: ApprovalCtx<'b>,
    ) -> BoxFuture<'b, ReviewDecision> {
        start_runtime_approval_async(
            self.approval_keys(req),
            RuntimeApprovalPlan {
                tool_name: "unified_exec",
                kind: RuntimeApprovalKind::UnifiedExec { tty: req.tty },
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

    fn exec_approval_requirement(
        &self,
        req: &UnifiedExecRequest,
    ) -> Option<ExecApprovalRequirement> {
        Some(req.exec_approval_requirement.clone())
    }

    fn sandbox_mode_for_first_attempt(&self, req: &UnifiedExecRequest) -> SandboxOverride {
        sandbox_override_for_first_attempt(req.sandbox_permissions, &req.exec_approval_requirement)
    }
}

impl<'a> ToolRuntime<UnifiedExecRequest, UnifiedExecProcess> for UnifiedExecRuntime<'a> {
    async fn preflight(
        &mut self,
        req: &UnifiedExecRequest,
        ctx: &ToolCtx,
    ) -> Result<(), ToolError> {
        preflight_command_intent(ctx, &req.command, &req.cwd).await
    }

    fn network_approval_spec(
        &self,
        req: &UnifiedExecRequest,
        _ctx: &ToolCtx,
    ) -> Option<NetworkApprovalSpec> {
        req.network.as_ref()?;
        Some(NetworkApprovalSpec {
            network: req.network.clone(),
            mode: NetworkApprovalMode::Deferred,
        })
    }

    async fn run(
        &mut self,
        req: &UnifiedExecRequest,
        attempt: &SandboxAttempt<'_>,
        ctx: &ToolCtx,
    ) -> Result<UnifiedExecProcess, ToolError> {
        let manager = self.manager;
        let runtime_owner_id = manager.runtime_owner_id();
        if let UnifiedExecShellMode::ZshFork(zsh_fork_config) = &self.shell_mode {
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
                    expiration: ExecExpiration::DefaultTimeout,
                    capture_policy: ExecCapturePolicy::ShellTool,
                    apply_managed_network_env: true,
                },
            )
            .map_err(|err| match err {
                ToolError::Rejected(_) => {
                    ToolError::Rejected("missing command line for PTY".to_string())
                }
                other => other,
            })?;
            match zsh_fork_backend::maybe_prepare_unified_exec(
                req,
                attempt,
                ctx,
                prepared.exec_request,
                zsh_fork_config,
            )
            .await?
            {
                Some(prepared) => {
                    if ctx.turn.environment.exec_server_url().is_some() {
                        return Err(ToolError::Rejected(
                            "unified_exec zsh-fork is not supported when exec_server_url is configured".to_string(),
                        ));
                    }
                    return run_process_open_with_agent_os(
                        ctx,
                        &req.command,
                        &req.cwd,
                        RuntimeExecutionRoute::unified_exec(runtime_owner_id, req.process_id),
                        move || async move {
                            manager
                                .open_session_with_exec_env(
                                    req.process_id,
                                    &prepared.exec_request,
                                    req.tty,
                                    prepared.spawn_lifecycle,
                                    ctx.turn.environment.as_ref(),
                                )
                                .await
                                .map_err(map_unified_exec_open_error)
                        },
                    )
                    .await;
                }
                None => {
                    tracing::warn!(
                        "UnifiedExec ZshFork backend specified, but conditions for using it were not met, falling back to direct execution",
                    );
                }
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
                expiration: ExecExpiration::DefaultTimeout,
                capture_policy: ExecCapturePolicy::ShellTool,
                apply_managed_network_env: true,
            },
        )
        .map_err(|err| match err {
            ToolError::Rejected(_) => {
                ToolError::Rejected("missing command line for PTY".to_string())
            }
            other => other,
        })?;
        run_process_open_with_agent_os(
            ctx,
            &req.command,
            &req.cwd,
            RuntimeExecutionRoute::unified_exec(runtime_owner_id, req.process_id),
            move || async move {
                manager
                    .open_session_with_exec_env(
                        req.process_id,
                        &prepared.exec_request,
                        req.tty,
                        Box::new(NoopSpawnLifecycle),
                        ctx.turn.environment.as_ref(),
                    )
                    .await
                    .map_err(map_unified_exec_open_error)
            },
        )
        .await
    }
}
fn map_unified_exec_open_error(err: UnifiedExecError) -> ToolError {
    match err {
        UnifiedExecError::SandboxDenied { output, .. } => {
            ToolError::Praxis(PraxisErr::Sandbox(SandboxErr::Denied {
                output: Box::new(output),
                network_policy_decision: None,
            }))
        }
        other => ToolError::Rejected(other.to_string()),
    }
}
