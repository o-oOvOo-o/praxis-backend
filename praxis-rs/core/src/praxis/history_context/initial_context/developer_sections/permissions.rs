use praxis_features::Feature;
use praxis_protocol::models::DeveloperInstructions;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) fn push_permission_policy(
    session: &Session,
    sections: &mut Vec<String>,
    turn_context: &TurnContext,
) {
    let permissions = turn_context.effective_permissions();
    sections.push(
        DeveloperInstructions::from_policy(
            permissions.sandbox_policy.get(),
            permissions.approval_policy.value(),
            turn_context.config.approvals_reviewer,
            session.services.exec_policy.current().as_ref(),
            &turn_context.cwd,
            turn_context
                .features
                .enabled(Feature::ExecPermissionApprovals),
            turn_context
                .features
                .enabled(Feature::RequestPermissionsTool),
        )
        .into_text(),
    );
}
