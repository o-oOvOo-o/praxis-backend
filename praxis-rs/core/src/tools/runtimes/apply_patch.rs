//! Apply Patch runtime: executes verified patches under the orchestrator.
//!
//! Assumes `apply_patch` verification/approval happened upstream. Reuses that
//! decision to avoid re-prompting, builds the self-invocation command for
//! `praxis --praxis-run-as-apply-patch`, and runs under the current
//! `SandboxAttempt` with a minimal environment.
use crate::exec::ExecCapturePolicy;
use crate::exec::ExecToolCallOutput;
use crate::guardian::GuardianApprovalRequest;
use crate::guardian::review_approval_request;
use crate::guardian::routes_approval_to_guardian;
use crate::sandboxing::ExecOptions;
use crate::tools::runtimes::managed_execution_pipeline::RuntimeExecutionRoute;
use crate::tools::runtimes::managed_execution_pipeline::preflight_command_intent;
use crate::tools::runtimes::managed_execution_pipeline::run_prestarted_one_shot_with_known_dirty_files;
use crate::tools::runtimes::managed_execution_pipeline::start_agent_os_span_for_route;
use crate::tools::sandboxing::Approvable;
use crate::tools::sandboxing::ApprovalCtx;
use crate::tools::sandboxing::ExecApprovalRequirement;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::Sandboxable;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use crate::tools::sandboxing::ToolRuntime;
use crate::tools::sandboxing::with_cached_approval;
use futures::future::BoxFuture;
use praxis_apply_patch::ApplyPatchAction;
use praxis_apply_patch::PRAXIS_RUN_AS_APPLY_PATCH_ARG1;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::FileChange;
use praxis_protocol::protocol::ReviewDecision;
use praxis_sandboxing::SandboxCommand;
use praxis_sandboxing::SandboxablePreference;
use praxis_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ApplyPatchRequest {
    pub action: ApplyPatchAction,
    pub agent_os_command: Vec<String>,
    pub file_paths: Vec<AbsolutePathBuf>,
    pub changes: std::collections::HashMap<PathBuf, FileChange>,
    pub exec_approval_requirement: ExecApprovalRequirement,
    pub additional_permissions: Option<PermissionProfile>,
    pub permissions_preapproved: bool,
    pub timeout_ms: Option<u64>,
}

#[derive(Default)]
pub struct ApplyPatchRuntime;

impl ApplyPatchRuntime {
    pub fn new() -> Self {
        Self
    }

    fn build_guardian_review_request(
        req: &ApplyPatchRequest,
        call_id: &str,
    ) -> GuardianApprovalRequest {
        GuardianApprovalRequest::ApplyPatch {
            id: call_id.to_string(),
            cwd: req.action.cwd.clone(),
            files: req.file_paths.clone(),
            patch: req.action.patch.clone(),
        }
    }

    #[cfg(target_os = "windows")]
    fn build_sandbox_command(
        req: &ApplyPatchRequest,
        praxis_home: &std::path::Path,
    ) -> Result<SandboxCommand, ToolError> {
        Ok(Self::build_sandbox_command_with_program(
            req,
            praxis_windows_sandbox::resolve_current_exe_for_launch(praxis_home, "praxis.exe"),
        ))
    }

    #[cfg(not(target_os = "windows"))]
    fn build_sandbox_command(
        req: &ApplyPatchRequest,
        praxis_self_exe: Option<&PathBuf>,
    ) -> Result<SandboxCommand, ToolError> {
        let exe = Self::resolve_apply_patch_program(praxis_self_exe)?;
        Ok(Self::build_sandbox_command_with_program(req, exe))
    }

    #[cfg(not(target_os = "windows"))]
    fn resolve_apply_patch_program(
        praxis_self_exe: Option<&PathBuf>,
    ) -> Result<PathBuf, ToolError> {
        if let Some(path) = praxis_self_exe {
            return Ok(path.clone());
        }

        std::env::current_exe()
            .map_err(|e| ToolError::Rejected(format!("failed to determine CLI executable: {e}")))
    }

    fn build_sandbox_command_with_program(req: &ApplyPatchRequest, exe: PathBuf) -> SandboxCommand {
        SandboxCommand {
            program: exe.into_os_string(),
            args: vec![
                PRAXIS_RUN_AS_APPLY_PATCH_ARG1.to_string(),
                req.action.patch.clone(),
            ],
            cwd: req.action.cwd.clone(),
            // Run apply_patch with a minimal environment for determinism and to avoid leaks.
            env: HashMap::new(),
            additional_permissions: req.additional_permissions.clone(),
        }
    }

    fn stdout_stream(ctx: &ToolCtx) -> Option<crate::exec::StdoutStream> {
        Some(crate::exec::StdoutStream {
            sub_id: ctx.turn.sub_id.clone(),
            call_id: ctx.call_id.clone(),
            tx_event: ctx.session.get_tx_event(),
        })
    }
}

pub(crate) fn apply_patch_agent_os_command(action: &ApplyPatchAction) -> Vec<String> {
    vec!["apply_patch".to_string(), action.patch.clone()]
}

impl Sandboxable for ApplyPatchRuntime {
    fn sandbox_preference(&self) -> SandboxablePreference {
        SandboxablePreference::Auto
    }
    fn escalate_on_failure(&self) -> bool {
        true
    }
}

impl Approvable<ApplyPatchRequest> for ApplyPatchRuntime {
    type ApprovalKey = AbsolutePathBuf;

    fn approval_keys(&self, req: &ApplyPatchRequest) -> Vec<Self::ApprovalKey> {
        req.file_paths.clone()
    }

    fn start_approval_async<'a>(
        &'a mut self,
        req: &'a ApplyPatchRequest,
        ctx: ApprovalCtx<'a>,
    ) -> BoxFuture<'a, ReviewDecision> {
        let session = ctx.session;
        let turn = ctx.turn;
        let call_id = ctx.call_id.to_string();
        let retry_reason = ctx.retry_reason.clone();
        let approval_keys = self.approval_keys(req);
        let changes = req.changes.clone();
        Box::pin(async move {
            if req.permissions_preapproved && retry_reason.is_none() {
                return ReviewDecision::Approved;
            }
            if routes_approval_to_guardian(turn) {
                let action = ApplyPatchRuntime::build_guardian_review_request(req, ctx.call_id);
                return review_approval_request(session, turn, action, retry_reason).await;
            }
            if let Some(reason) = retry_reason {
                let rx_approve = session
                    .request_patch_approval(
                        turn,
                        call_id,
                        changes.clone(),
                        Some(reason),
                        /*grant_root*/ None,
                    )
                    .await;
                return rx_approve.await.unwrap_or_default();
            }

            with_cached_approval(
                &session.services,
                "apply_patch",
                approval_keys,
                || async move {
                    let rx_approve = session
                        .request_patch_approval(
                            turn, call_id, changes, /*reason*/ None, /*grant_root*/ None,
                        )
                        .await;
                    rx_approve.await.unwrap_or_default()
                },
            )
            .await
        })
    }

    fn wants_no_sandbox_approval(&self, policy: AskForApproval) -> bool {
        match policy {
            AskForApproval::Never => false,
            AskForApproval::Granular(granular_config) => granular_config.allows_sandbox_approval(),
            AskForApproval::OnFailure => true,
            AskForApproval::OnRequest => true,
            AskForApproval::UnlessTrusted => true,
        }
    }

    // apply_patch approvals are decided upstream by assess_patch_safety.
    //
    // This override ensures the orchestrator runs the patch approval flow when required instead
    // of falling back to the global exec approval policy.
    fn exec_approval_requirement(
        &self,
        req: &ApplyPatchRequest,
    ) -> Option<ExecApprovalRequirement> {
        Some(req.exec_approval_requirement.clone())
    }
}

impl ToolRuntime<ApplyPatchRequest, ExecToolCallOutput> for ApplyPatchRuntime {
    async fn preflight(&mut self, req: &ApplyPatchRequest, ctx: &ToolCtx) -> Result<(), ToolError> {
        preflight_command_intent(ctx, &req.agent_os_command, &req.action.cwd).await
    }

    async fn run(
        &mut self,
        req: &ApplyPatchRequest,
        attempt: &SandboxAttempt<'_>,
        ctx: &ToolCtx,
    ) -> Result<ExecToolCallOutput, ToolError> {
        #[cfg(target_os = "windows")]
        let command = Self::build_sandbox_command(req, &ctx.turn.config.praxis_home)?;
        #[cfg(not(target_os = "windows"))]
        let command = Self::build_sandbox_command(req, ctx.turn.praxis_self_exe.as_ref())?;
        let options = ExecOptions {
            expiration: req.timeout_ms.into(),
            capture_policy: ExecCapturePolicy::ShellTool,
        };
        let env = attempt
            .env_for(command, options, /*network*/ None)
            .map_err(|err| ToolError::Praxis(err.into()))?;
        let command_span = start_agent_os_span_for_route(
            ctx,
            &req.agent_os_command,
            &req.action.cwd,
            RuntimeExecutionRoute::apply_patch(),
        )
        .await?;
        let dirty_files = req
            .file_paths
            .iter()
            .map(|path| path.clone().into_path_buf())
            .collect();
        run_prestarted_one_shot_with_known_dirty_files(
            &command_span,
            RuntimeExecutionRoute::apply_patch(),
            dirty_files,
            env,
            Self::stdout_stream(ctx),
        )
        .await
    }
}

#[cfg(test)]
#[path = "apply_patch_tests.rs"]
mod tests;
