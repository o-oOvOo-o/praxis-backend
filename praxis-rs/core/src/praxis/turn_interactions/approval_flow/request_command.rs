use std::path::PathBuf;

use praxis_protocol::approvals::ExecPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyAmendment;
use praxis_protocol::approvals::NetworkPolicyRuleAction;
use praxis_protocol::models::PermissionProfile;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::NetworkApprovalContext;
use praxis_protocol::protocol::ReviewDecision;
use praxis_shell_command::parse_command::parse_command;
use tokio::sync::oneshot;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::pending::insert_pending_approval;

impl Session {
    /// Emit an exec approval request event and await the user's decision.
    #[allow(clippy::too_many_arguments)]
    pub async fn request_command_approval(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        approval_id: Option<String>,
        command: Vec<String>,
        cwd: PathBuf,
        reason: Option<String>,
        network_approval_context: Option<NetworkApprovalContext>,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
        additional_permissions: Option<PermissionProfile>,
        available_decisions: Option<Vec<ReviewDecision>>,
    ) -> ReviewDecision {
        let effective_approval_id = approval_id.clone().unwrap_or_else(|| call_id.clone());
        let (tx_approve, rx_approve) = oneshot::channel();
        insert_pending_approval(self, effective_approval_id, tx_approve).await;

        let parsed_cmd = parse_command(&command);
        let proposed_network_policy_amendments =
            proposed_network_policy_amendments(network_approval_context.as_ref());
        let available_decisions = available_decisions.unwrap_or_else(|| {
            ExecApprovalRequestEvent::default_available_decisions(
                network_approval_context.as_ref(),
                proposed_execpolicy_amendment.as_ref(),
                proposed_network_policy_amendments.as_deref(),
                additional_permissions.as_ref(),
            )
        });
        let event = EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
            call_id,
            approval_id,
            turn_id: turn_context.sub_id.clone(),
            command,
            cwd,
            reason,
            network_approval_context,
            proposed_execpolicy_amendment,
            proposed_network_policy_amendments,
            additional_permissions,
            available_decisions: Some(available_decisions),
            parsed_cmd,
        });
        self.send_event(turn_context, event).await;
        rx_approve.await.unwrap_or(ReviewDecision::Abort)
    }
}

fn proposed_network_policy_amendments(
    network_approval_context: Option<&NetworkApprovalContext>,
) -> Option<Vec<NetworkPolicyAmendment>> {
    network_approval_context.map(|context| {
        vec![
            NetworkPolicyAmendment {
                host: context.host.clone(),
                action: NetworkPolicyRuleAction::Allow,
            },
            NetworkPolicyAmendment {
                host: context.host.clone(),
                action: NetworkPolicyRuleAction::Deny,
            },
        ]
    })
}
