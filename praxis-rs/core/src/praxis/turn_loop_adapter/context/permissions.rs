use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;

use super::super::super::TurnContext;

pub(super) fn build_permissions(
    turn_context: &TurnContext,
) -> praxis_loop::context::EffectivePermissions {
    let permissions = turn_context.effective_permissions();
    let sandbox_policy = permissions.sandbox_policy.get();
    praxis_loop::context::EffectivePermissions {
        write: sandbox_policy.has_full_disk_write_access()
            || matches!(sandbox_policy, SandboxPolicy::WorkspaceWrite { .. }),
        network: sandbox_policy.has_full_network_access(),
        approval_required: !matches!(permissions.approval_policy.value(), AskForApproval::Never),
    }
}
