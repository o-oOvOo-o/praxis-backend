use std::path::Path;

use praxis_features::Feature;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::protocol::AskForApproval;

use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::sandboxing::SandboxPermissions;

use super::EffectiveAdditionalPermissions;
use super::apply_granted_turn_permissions;
use super::implicit_granted_permissions;
use super::normalize_and_validate_additional_permissions;

/// Normalized execution permissions shared by shell, unified exec, and future
/// command-like runtimes.
///
/// Keep this as the single place that resolves sticky turn permissions,
/// explicit escalation requests, and `with_additional_permissions` validation.
/// Handler-specific code should only build a request/runtime; it should not
/// re-implement permission policy.
pub(crate) struct ManagedExecutionPermissions {
    pub(crate) effective: EffectiveAdditionalPermissions,
    pub(crate) normalized_additional_permissions: Option<PermissionProfile>,
}

pub(crate) fn prepare_managed_execution_permissions(
    session: &Session,
    sandbox_permissions: SandboxPermissions,
    requested_additional_permissions: Option<PermissionProfile>,
    approval_policy: AskForApproval,
    cwd: &Path,
) -> Result<ManagedExecutionPermissions, FunctionCallError> {
    let exec_permission_approvals_enabled =
        session.features().enabled(Feature::ExecPermissionApprovals);
    let effective = apply_granted_turn_permissions(
        session,
        sandbox_permissions,
        requested_additional_permissions.clone(),
    );
    let additional_permissions_allowed = exec_permission_approvals_enabled
        || (session.features().enabled(Feature::RequestPermissionsTool)
            && effective.permissions_preapproved);

    reject_disallowed_explicit_escalation(&effective, approval_policy)?;

    let normalized_additional_permissions = implicit_granted_permissions(
        sandbox_permissions,
        requested_additional_permissions.as_ref(),
        &effective,
    )
    .map_or_else(
        || {
            normalize_and_validate_additional_permissions(
                additional_permissions_allowed,
                approval_policy,
                effective.sandbox_permissions,
                effective.additional_permissions.clone(),
                effective.permissions_preapproved,
                cwd,
            )
        },
        |permissions| Ok(Some(permissions)),
    )
    .map_err(FunctionCallError::RespondToModel)?;

    Ok(ManagedExecutionPermissions {
        effective,
        normalized_additional_permissions,
    })
}

fn reject_disallowed_explicit_escalation(
    effective: &EffectiveAdditionalPermissions,
    approval_policy: AskForApproval,
) -> Result<(), FunctionCallError> {
    // Sticky turn/session permissions are already approved and should continue
    // through the regular exec approval flow. Fresh explicit escalation in a
    // non-OnRequest policy is rejected before any AgentOS ticket/lease/process
    // allocation occurs.
    if effective.sandbox_permissions.requests_sandbox_override()
        && !effective.permissions_preapproved
        && !matches!(approval_policy, AskForApproval::OnRequest)
    {
        return Err(FunctionCallError::RespondToModel(format!(
            "approval policy is {approval_policy:?}; reject command — you cannot ask for escalated permissions if the approval policy is {approval_policy:?}"
        )));
    }

    Ok(())
}
