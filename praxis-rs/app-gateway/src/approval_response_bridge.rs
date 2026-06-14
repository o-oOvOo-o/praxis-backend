use crate::client_response_decode::ClientResponseValue;
use crate::client_response_decode::PendingClientResponse;
use crate::client_response_decode::decode_response_value_or_default;
use crate::client_response_decode::response_value_or_cancel;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalResponse;
use praxis_app_gateway_protocol::CommandExecutionStatus;
use praxis_app_gateway_protocol::FileChangeApprovalDecision;
use praxis_app_gateway_protocol::FileChangeRequestApprovalResponse;
use praxis_app_gateway_protocol::NetworkPolicyRuleAction;
use praxis_app_gateway_protocol::PatchApplyStatus;
use praxis_protocol::protocol::ReviewDecision;

pub(crate) fn file_change_approval_response_outcome(
    response: PendingClientResponse,
) -> Option<(ReviewDecision, Option<PatchApplyStatus>)> {
    match response_value_or_cancel(response) {
        ClientResponseValue::Value(value) => {
            let response = decode_response_value_or_default::<FileChangeRequestApprovalResponse>(
                value,
                || FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Decline,
                },
            );
            Some(map_file_change_approval_decision(response.decision))
        }
        ClientResponseValue::TurnTransition => None,
        ClientResponseValue::Fallback => {
            Some((ReviewDecision::Denied, Some(PatchApplyStatus::Failed)))
        }
    }
}

pub(crate) fn map_file_change_approval_decision(
    decision: FileChangeApprovalDecision,
) -> (ReviewDecision, Option<PatchApplyStatus>) {
    match decision {
        FileChangeApprovalDecision::Accept => (ReviewDecision::Approved, None),
        FileChangeApprovalDecision::AcceptForSession => (ReviewDecision::ApprovedForSession, None),
        FileChangeApprovalDecision::Decline => {
            (ReviewDecision::Denied, Some(PatchApplyStatus::Declined))
        }
        FileChangeApprovalDecision::Cancel => {
            (ReviewDecision::Abort, Some(PatchApplyStatus::Declined))
        }
    }
}

pub(crate) fn command_execution_approval_response_outcome(
    response: PendingClientResponse,
) -> Option<(ReviewDecision, Option<CommandExecutionStatus>)> {
    match response_value_or_cancel(response) {
        ClientResponseValue::Value(value) => {
            let response = decode_response_value_or_default::<
                CommandExecutionRequestApprovalResponse,
            >(value, || CommandExecutionRequestApprovalResponse {
                decision: CommandExecutionApprovalDecision::Decline,
            });
            Some(map_command_execution_approval_decision(response.decision))
        }
        ClientResponseValue::TurnTransition => None,
        ClientResponseValue::Fallback => {
            Some((ReviewDecision::Denied, Some(CommandExecutionStatus::Failed)))
        }
    }
}

pub(crate) fn map_command_execution_approval_decision(
    decision: CommandExecutionApprovalDecision,
) -> (ReviewDecision, Option<CommandExecutionStatus>) {
    match decision {
        CommandExecutionApprovalDecision::Accept => (ReviewDecision::Approved, None),
        CommandExecutionApprovalDecision::AcceptForSession => {
            (ReviewDecision::ApprovedForSession, None)
        }
        CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment {
            execpolicy_amendment,
        } => (
            ReviewDecision::ApprovedExecpolicyAmendment {
                proposed_execpolicy_amendment: execpolicy_amendment.into_core(),
            },
            None,
        ),
        CommandExecutionApprovalDecision::ApplyNetworkPolicyAmendment {
            network_policy_amendment,
        } => {
            let completion_status = match network_policy_amendment.action {
                NetworkPolicyRuleAction::Allow => None,
                NetworkPolicyRuleAction::Deny => Some(CommandExecutionStatus::Declined),
            };
            (
                ReviewDecision::NetworkPolicyAmendment {
                    network_policy_amendment: network_policy_amendment.into_core(),
                },
                completion_status,
            )
        }
        CommandExecutionApprovalDecision::Decline => (
            ReviewDecision::Denied,
            Some(CommandExecutionStatus::Declined),
        ),
        CommandExecutionApprovalDecision::Cancel => (
            ReviewDecision::Abort,
            Some(CommandExecutionStatus::Declined),
        ),
    }
}
