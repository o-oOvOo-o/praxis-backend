/*
Module: orchestrator

Central place for approvals + sandbox selection + retry semantics. Drives a
simple sequence for any ToolRuntime: approval → select sandbox → attempt →
retry with an escalated sandbox strategy on denial (no re‑approval thanks to
caching).
*/
use crate::error::PraxisErr;
use crate::error::SandboxErr;
use crate::exec::ExecToolCallOutput;
use crate::guardian::GUARDIAN_REJECTION_MESSAGE;
use crate::guardian::routes_approval_to_guardian;
use crate::network_policy_decision::network_approval_context_from_payload;
use crate::tools::network_approval::DeferredNetworkApproval;
use crate::tools::network_approval::NetworkApprovalMode;
use crate::tools::network_approval::begin_network_approval;
use crate::tools::network_approval::finish_deferred_network_approval;
use crate::tools::network_approval::finish_immediate_network_approval;
use crate::tools::sandboxing::ApprovalCtx;
use crate::tools::sandboxing::ExecApprovalRequirement;
use crate::tools::sandboxing::SandboxAttempt;
use crate::tools::sandboxing::SandboxOverride;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use crate::tools::sandboxing::ToolRuntime;
use crate::tools::sandboxing::default_exec_approval_requirement;
use praxis_otel::ToolDecisionSource;
use praxis_protocol::protocol::NetworkPolicyRuleAction;
use praxis_protocol::protocol::ReviewDecision;
use praxis_sandboxing::SandboxManager;
use praxis_sandboxing::SandboxType;
use praxis_system_plugin_approval_control::ApprovalDecision;
use praxis_system_plugin_approval_control::tool_safety::SandboxRetryPolicy;
use praxis_system_plugin_approval_control::tool_safety::SandboxRetryRequest;
use praxis_system_plugin_approval_control::tool_safety::ToolSafetyOrchestrator;
use praxis_system_plugin_approval_control::tool_safety::ToolSafetyRequest;

pub(crate) struct ToolOrchestrator {
    sandbox: SandboxManager,
}

pub(crate) struct OrchestratorRunResult<Out> {
    pub output: Out,
    pub deferred_network_approval: Option<DeferredNetworkApproval>,
}

impl ToolOrchestrator {
    pub fn new() -> Self {
        Self {
            sandbox: SandboxManager::new(),
        }
    }

    async fn run_attempt<Rq, Out, T>(
        tool: &mut T,
        req: &Rq,
        tool_ctx: &ToolCtx,
        attempt: &SandboxAttempt<'_>,
        has_managed_network_requirements: bool,
    ) -> (Result<Out, ToolError>, Option<DeferredNetworkApproval>)
    where
        T: ToolRuntime<Rq, Out>,
    {
        let network_approval = begin_network_approval(
            &tool_ctx.session,
            &tool_ctx.turn.sub_id,
            has_managed_network_requirements,
            tool.network_approval_spec(req, tool_ctx),
        )
        .await;

        let attempt_tool_ctx = ToolCtx {
            session: tool_ctx.session.clone(),
            turn: tool_ctx.turn.clone(),
            call_id: tool_ctx.call_id.clone(),
            tool_name: tool_ctx.tool_name.clone(),
        };
        let run_result = tool.run(req, attempt, &attempt_tool_ctx).await;

        let Some(network_approval) = network_approval else {
            return (run_result, None);
        };

        match network_approval.mode() {
            NetworkApprovalMode::Immediate => {
                let finalize_result =
                    finish_immediate_network_approval(&tool_ctx.session, network_approval).await;
                if let Err(err) = finalize_result {
                    return (Err(err), None);
                }
                (run_result, None)
            }
            NetworkApprovalMode::Deferred => {
                let deferred = network_approval.into_deferred();
                if run_result.is_err() {
                    finish_deferred_network_approval(&tool_ctx.session, deferred).await;
                    return (run_result, None);
                }
                (run_result, deferred)
            }
        }
    }

    pub async fn run<Rq, Out, T>(
        &mut self,
        tool: &mut T,
        req: &Rq,
        tool_ctx: &ToolCtx,
        turn_ctx: &crate::praxis::TurnContext,
    ) -> Result<OrchestratorRunResult<Out>, ToolError>
    where
        T: ToolRuntime<Rq, Out>,
    {
        let permissions = turn_ctx.effective_permissions();
        let resolved_permissions = permissions.as_resolved_turn_permissions().normalized();
        let approval_policy = resolved_permissions.approval_policy;
        let tool_kind = tool.tool_kind();
        let otel = turn_ctx.session_telemetry.clone();
        let otel_tn = &tool_ctx.tool_name;
        let otel_ci = &tool_ctx.call_id;
        let otel_user = ToolDecisionSource::User;
        let otel_automated_reviewer = ToolDecisionSource::AutomatedReviewer;
        let otel_cfg = ToolDecisionSource::Config;

        // 1) Approval
        let mut already_approved = false;

        tool.preflight(req, tool_ctx).await?;

        let requirement = tool.exec_approval_requirement(req).unwrap_or_else(|| {
            default_exec_approval_requirement(
                approval_policy,
                &resolved_permissions.file_system_sandbox_policy,
            )
        });
        let safety = ToolSafetyOrchestrator;
        let safety_decision = match &requirement {
            ExecApprovalRequirement::Skip { .. } => safety.decide(ToolSafetyRequest {
                id: &tool_ctx.call_id,
                thread_id: None,
                turn_id: Some(&tool_ctx.turn.sub_id),
                kind: tool_kind,
                permissions: &resolved_permissions,
                approval_required: false,
                reason: None,
            }),
            ExecApprovalRequirement::Forbidden { reason } => ApprovalDecision::deny(reason.clone()),
            ExecApprovalRequirement::NeedsApproval { reason, .. } => {
                safety.decide(ToolSafetyRequest {
                    id: &tool_ctx.call_id,
                    thread_id: None,
                    turn_id: Some(&tool_ctx.turn.sub_id),
                    kind: tool_kind,
                    permissions: &resolved_permissions,
                    approval_required: true,
                    reason: reason.as_deref(),
                })
            }
        };

        match safety_decision {
            ApprovalDecision::Run { .. } => {
                otel.tool_decision(otel_tn, otel_ci, &ReviewDecision::Approved, otel_cfg);
                if matches!(requirement, ExecApprovalRequirement::NeedsApproval { .. }) {
                    already_approved = true;
                }
            }
            ApprovalDecision::Deny { reason } => {
                return Err(ToolError::Rejected(reason));
            }
            ApprovalDecision::AskUser { request } => {
                let approval_ctx = ApprovalCtx {
                    session: &tool_ctx.session,
                    turn: &tool_ctx.turn,
                    call_id: &tool_ctx.call_id,
                    retry_reason: request.reason,
                    network_approval_context: None,
                };
                let decision = tool.start_approval_async(req, approval_ctx).await;
                let routed_to_guardian = routes_approval_to_guardian(turn_ctx);
                let otel_source = if routed_to_guardian {
                    otel_automated_reviewer.clone()
                } else {
                    otel_user.clone()
                };

                otel.tool_decision(otel_tn, otel_ci, &decision, otel_source);
                apply_review_decision(decision, routed_to_guardian)?;
                already_approved = true;
            }
        }

        // 2) First attempt under the selected sandbox.
        let has_managed_network_requirements = turn_ctx
            .config
            .config_layer_stack
            .requirements_toml()
            .network
            .is_some();
        let initial_sandbox = match tool.sandbox_mode_for_first_attempt(req) {
            SandboxOverride::BypassSandboxFirstAttempt => SandboxType::None,
            SandboxOverride::NoOverride => self.sandbox.select_initial(
                &resolved_permissions.file_system_sandbox_policy,
                resolved_permissions.network_sandbox_policy,
                tool.sandbox_preference(),
                resolved_permissions.windows_sandbox_level,
                has_managed_network_requirements,
            ),
        };

        // Platform-specific flag gating is handled by SandboxManager::select_initial.
        let use_legacy_landlock = turn_ctx.features.use_legacy_landlock();
        let initial_attempt = SandboxAttempt {
            sandbox: initial_sandbox,
            policy: &resolved_permissions.sandbox_policy,
            file_system_policy: &resolved_permissions.file_system_sandbox_policy,
            network_policy: resolved_permissions.network_sandbox_policy,
            enforce_managed_network: has_managed_network_requirements,
            manager: &self.sandbox,
            sandbox_cwd: &turn_ctx.cwd,
            praxis_linux_sandbox_exe: turn_ctx.praxis_linux_sandbox_exe.as_ref(),
            use_legacy_landlock,
            windows_sandbox_level: resolved_permissions.windows_sandbox_level,
            windows_sandbox_private_desktop: turn_ctx
                .config
                .permissions
                .windows_sandbox_private_desktop,
        };

        let (first_result, first_deferred_network_approval) = Self::run_attempt(
            tool,
            req,
            tool_ctx,
            &initial_attempt,
            has_managed_network_requirements,
        )
        .await;
        match first_result {
            Ok(out) => {
                // We have a successful initial result
                Ok(OrchestratorRunResult {
                    output: out,
                    deferred_network_approval: first_deferred_network_approval,
                })
            }
            Err(ToolError::Praxis(PraxisErr::Sandbox(SandboxErr::Denied {
                output,
                network_policy_decision,
            }))) => {
                let network_approval_context = if has_managed_network_requirements {
                    network_policy_decision
                        .as_ref()
                        .and_then(network_approval_context_from_payload)
                } else {
                    None
                };
                if network_policy_decision.is_some() && network_approval_context.is_none() {
                    return Err(ToolError::Praxis(PraxisErr::Sandbox(SandboxErr::Denied {
                        output,
                        network_policy_decision,
                    })));
                }
                if !tool.escalate_on_failure() {
                    return Err(ToolError::Praxis(PraxisErr::Sandbox(SandboxErr::Denied {
                        output,
                        network_policy_decision,
                    })));
                }
                let retry_permissions = turn_ctx.effective_permissions();
                let retry_resolved_permissions = retry_permissions
                    .as_resolved_turn_permissions()
                    .normalized();
                let retry_approval_policy = retry_resolved_permissions.approval_policy;
                let retry_policy = SandboxRetryPolicy::for_denied_sandbox(SandboxRetryRequest {
                    kind: tool_kind,
                    permissions: &retry_resolved_permissions,
                    tool_allows_no_sandbox_approval: tool
                        .wants_no_sandbox_approval(retry_approval_policy),
                    tool_bypasses_retry_approval: tool
                        .should_bypass_approval(retry_approval_policy, already_approved),
                    network_retry_available: network_approval_context.is_some(),
                    network_retry_requires_approval: matches!(
                        default_exec_approval_requirement(
                            retry_approval_policy,
                            &retry_resolved_permissions.file_system_sandbox_policy
                        ),
                        ExecApprovalRequirement::NeedsApproval { .. }
                    ),
                });
                if !retry_policy.allow_without_sandbox {
                    return Err(ToolError::Praxis(PraxisErr::Sandbox(SandboxErr::Denied {
                        output,
                        network_policy_decision,
                    })));
                }
                let retry_reason =
                    if let Some(network_approval_context) = network_approval_context.as_ref() {
                        format!(
                            "Network access to \"{}\" is blocked by policy.",
                            network_approval_context.host
                        )
                    } else {
                        build_denial_reason_from_output(output.as_ref())
                    };

                if retry_policy.ask_before_retry {
                    let approval_ctx = ApprovalCtx {
                        session: &tool_ctx.session,
                        turn: &tool_ctx.turn,
                        call_id: &tool_ctx.call_id,
                        retry_reason: Some(retry_reason),
                        network_approval_context: network_approval_context.clone(),
                    };

                    let decision = tool.start_approval_async(req, approval_ctx).await;
                    let routed_to_guardian = routes_approval_to_guardian(turn_ctx);
                    let otel_source = if routed_to_guardian {
                        otel_automated_reviewer
                    } else {
                        otel_user
                    };
                    otel.tool_decision(otel_tn, otel_ci, &decision, otel_source);
                    apply_review_decision(decision, routed_to_guardian)?;
                }

                let escalated_attempt = SandboxAttempt {
                    sandbox: SandboxType::None,
                    policy: &retry_resolved_permissions.sandbox_policy,
                    file_system_policy: &retry_resolved_permissions.file_system_sandbox_policy,
                    network_policy: retry_resolved_permissions.network_sandbox_policy,
                    enforce_managed_network: has_managed_network_requirements,
                    manager: &self.sandbox,
                    sandbox_cwd: &turn_ctx.cwd,
                    praxis_linux_sandbox_exe: None,
                    use_legacy_landlock,
                    windows_sandbox_level: retry_resolved_permissions.windows_sandbox_level,
                    windows_sandbox_private_desktop: turn_ctx
                        .config
                        .permissions
                        .windows_sandbox_private_desktop,
                };

                tool.preflight(req, tool_ctx).await?;

                // Second attempt.
                let (retry_result, retry_deferred_network_approval) = Self::run_attempt(
                    tool,
                    req,
                    tool_ctx,
                    &escalated_attempt,
                    has_managed_network_requirements,
                )
                .await;
                retry_result.map(|output| OrchestratorRunResult {
                    output,
                    deferred_network_approval: retry_deferred_network_approval,
                })
            }
            Err(err) => Err(err),
        }
    }
}

fn build_denial_reason_from_output(_output: &ExecToolCallOutput) -> String {
    // Keep approval reason terse and stable for UX/tests, but accept the
    // output so we can evolve heuristics later without touching call sites.
    "command failed; retry without sandbox?".to_string()
}

fn apply_review_decision(
    decision: ReviewDecision,
    routed_to_guardian: bool,
) -> Result<(), ToolError> {
    match decision {
        ReviewDecision::Denied | ReviewDecision::Abort => {
            let reason = if routed_to_guardian {
                GUARDIAN_REJECTION_MESSAGE.to_string()
            } else {
                "rejected by user".to_string()
            };
            Err(ToolError::Rejected(reason))
        }
        ReviewDecision::Approved
        | ReviewDecision::ApprovedExecpolicyAmendment { .. }
        | ReviewDecision::ApprovedForSession => Ok(()),
        ReviewDecision::NetworkPolicyAmendment {
            network_policy_amendment,
        } => match network_policy_amendment.action {
            NetworkPolicyRuleAction::Allow => Ok(()),
            NetworkPolicyRuleAction::Deny => {
                Err(ToolError::Rejected("rejected by user".to_string()))
            }
        },
    }
}
