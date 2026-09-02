use crate::approval_response_bridge::command_execution_approval_response_outcome;
use crate::approval_response_bridge::file_change_approval_response_outcome;
use crate::automation_projection::api_automation_run_from_state;
use crate::client_response_decode::ClientResponseValue;
use crate::client_response_decode::PendingClientResponse;
use crate::client_response_decode::decode_response_value_or_default;
use crate::client_response_decode::response_value_or_cancel;
use crate::client_response_decode::try_decode_client_response_or_default;
use crate::collab_agent_event_bridge::collab_agent_status_failed;
use crate::collab_agent_event_bridge::collab_close_begin_item;
use crate::collab_agent_event_bridge::collab_close_end_item;
use crate::collab_agent_event_bridge::collab_interaction_begin_item;
use crate::collab_agent_event_bridge::collab_interaction_end_item;
use crate::collab_agent_event_bridge::collab_resume_begin_item;
use crate::collab_agent_event_bridge::collab_resume_end_item;
use crate::collab_agent_event_bridge::collab_spawn_begin_item;
use crate::collab_agent_event_bridge::collab_spawn_end_item;
use crate::collab_agent_event_bridge::collab_waiting_begin_item;
use crate::collab_agent_event_bridge::collab_waiting_end_item;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::mcp_tool_event_bridge::construct_mcp_tool_call_end_item;
use crate::mcp_tool_event_bridge::construct_mcp_tool_call_item;
use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use crate::praxis_message_processor::project_rollback_thread_from_rollout;
use crate::praxis_message_processor::thread_selfwork_api::advance_selfwork_after_turn;
use crate::realtime_event_bridge::send_realtime_closed;
use crate::realtime_event_bridge::send_realtime_event;
use crate::realtime_event_bridge::send_realtime_started;
use crate::server_request_lifecycle::PendingServerRequest;
use crate::server_request_lifecycle::send_server_request;
use crate::thread_item_event_bridge::ThreadItemNotificationSink;
use crate::thread_state::ThreadState;
use crate::thread_state::ThreadStateManager;
use crate::thread_state::TurnSummary;
use crate::thread_status::ThreadWatchActiveGuard;
use crate::thread_status::ThreadWatchManager;
use crate::workspace_change_store::WorkspaceChangeStore;
use praxis_app_gateway_protocol::AccountRateLimitsUpdatedNotification;
use praxis_app_gateway_protocol::AdditionalPermissionProfile as ApiAdditionalPermissionProfile;
use praxis_app_gateway_protocol::AgentMessageDeltaNotification;
use praxis_app_gateway_protocol::AutomationRunUpdatedNotification;
use praxis_app_gateway_protocol::CommandAction as ApiParsedCommand;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalParams;
use praxis_app_gateway_protocol::CommandExecutionSource;
use praxis_app_gateway_protocol::CommandExecutionStatus;
use praxis_app_gateway_protocol::DeprecationNoticeNotification;
use praxis_app_gateway_protocol::DynamicToolCallOutputContentItem;
use praxis_app_gateway_protocol::DynamicToolCallParams;
use praxis_app_gateway_protocol::DynamicToolCallStatus;
use praxis_app_gateway_protocol::ErrorNotification;
use praxis_app_gateway_protocol::ExecPolicyAmendment as ApiExecPolicyAmendment;
use praxis_app_gateway_protocol::FileChangeOutputDeltaNotification;
use praxis_app_gateway_protocol::FileChangeRequestApprovalParams;
use praxis_app_gateway_protocol::FileUpdateChange;
use praxis_app_gateway_protocol::GrantedPermissionProfile as ApiGrantedPermissionProfile;
use praxis_app_gateway_protocol::GuardianApprovalReview;
use praxis_app_gateway_protocol::GuardianApprovalReviewStatus;
use praxis_app_gateway_protocol::HookCompletedNotification;
use praxis_app_gateway_protocol::HookStartedNotification;
use praxis_app_gateway_protocol::ItemGuardianApprovalReviewCompletedNotification;
use praxis_app_gateway_protocol::ItemGuardianApprovalReviewStartedNotification;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::McpServerElicitationAction;
use praxis_app_gateway_protocol::McpServerElicitationRequestParams;
use praxis_app_gateway_protocol::McpServerElicitationRequestResponse;
use praxis_app_gateway_protocol::McpServerStartupState;
use praxis_app_gateway_protocol::McpServerStatusUpdatedNotification;
use praxis_app_gateway_protocol::ModelReroutedNotification;
use praxis_app_gateway_protocol::NetworkApprovalContext as ApiNetworkApprovalContext;
use praxis_app_gateway_protocol::NetworkPolicyAmendment as ApiNetworkPolicyAmendment;
use praxis_app_gateway_protocol::PatchApplyStatus;
use praxis_app_gateway_protocol::PermissionsRequestApprovalParams;
use praxis_app_gateway_protocol::PermissionsRequestApprovalResponse;
use praxis_app_gateway_protocol::PlanDeltaNotification;
use praxis_app_gateway_protocol::PraxisErrorInfo as ApiPraxisErrorInfo;
use praxis_app_gateway_protocol::RawResponseItemCompletedNotification;
use praxis_app_gateway_protocol::ReasoningSummaryPartAddedNotification;
use praxis_app_gateway_protocol::ReasoningSummaryTextDeltaNotification;
use praxis_app_gateway_protocol::ReasoningTextDeltaNotification;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::ServerRequestPayload;
use praxis_app_gateway_protocol::SkillsChangedNotification;
use praxis_app_gateway_protocol::TerminalInteractionNotification;
use praxis_app_gateway_protocol::ThreadClosedNotification;
use praxis_app_gateway_protocol::ThreadController;
use praxis_app_gateway_protocol::ThreadControllerKind;
use praxis_app_gateway_protocol::ThreadGoalUpdatedNotification;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadNameUpdatedNotification;
use praxis_app_gateway_protocol::ThreadRollbackResponse;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_app_gateway_protocol::ThreadTokenUsageUpdatedNotification;
use praxis_app_gateway_protocol::ThreadWorkspaceRestoreResult;
use praxis_app_gateway_protocol::ToolRequestUserInputOption;
use praxis_app_gateway_protocol::ToolRequestUserInputParams;
use praxis_app_gateway_protocol::ToolRequestUserInputQuestion;
use praxis_app_gateway_protocol::ToolRequestUserInputResponse;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnCompletedNotification;
use praxis_app_gateway_protocol::TurnDiffUpdatedNotification;
use praxis_app_gateway_protocol::TurnError;
use praxis_app_gateway_protocol::TurnInterruptResponse;
use praxis_app_gateway_protocol::TurnPlanStep;
use praxis_app_gateway_protocol::TurnPlanUpdatedNotification;
use praxis_app_gateway_protocol::TurnStartedNotification;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::WorkspaceChangeUpdatedNotification;
use praxis_app_gateway_protocol::convert_patch_changes;
use praxis_core::PraxisThread;
use praxis_core::ThreadManager;
use praxis_core::review_format::REVIEW_FALLBACK_MESSAGE;
use praxis_core::review_format::render_review_output_text;
use praxis_core::review_prompts;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem as CoreDynamicToolCallOutputContentItem;
use praxis_protocol::items::parse_hook_prompt_message;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::GuardianAssessmentEvent;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::PraxisErrorInfo as CorePraxisErrorInfo;
use praxis_protocol::protocol::TokenCountEvent;
use praxis_protocol::protocol::TurnDiffEvent;
use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsResponse as CoreRequestPermissionsResponse;
use praxis_protocol::request_user_input::RequestUserInputAnswer as CoreRequestUserInputAnswer;
use praxis_protocol::request_user_input::RequestUserInputResponse as CoreRequestUserInputResponse;
use praxis_rollout::ThreadHistoryReader;
use praxis_sandboxing::policy_transforms::intersect_permission_profiles;
use praxis_shell_command::parse_command::shlex_join;
use praxis_state::AutomationRunStatus;
use praxis_state::StateRuntime;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::error;

mod approval_requests;
mod dispatch;
#[cfg(test)]
mod tests;
mod turn_handlers;
pub(crate) use dispatch::apply_bespoke_event_handling;

use approval_requests::handle_apply_patch_approval_request;
use approval_requests::handle_exec_approval_request;
use turn_handlers::complete_file_change_item;
use turn_handlers::finish_automation_runs_for_turn;
use turn_handlers::handle_error;
use turn_handlers::handle_thread_rollback_failed;
use turn_handlers::handle_token_count_event;
use turn_handlers::handle_turn_complete;
use turn_handlers::handle_turn_diff;
use turn_handlers::handle_turn_interrupted;
use turn_handlers::handle_turn_plan_update;
use turn_handlers::maybe_emit_hook_prompt_item_completed;
use turn_handlers::maybe_emit_raw_response_item_completed;
use turn_handlers::on_command_execution_request_approval_response;
use turn_handlers::on_file_change_request_approval_response;
use turn_handlers::on_mcp_server_elicitation_response;
use turn_handlers::on_request_permissions_response;
use turn_handlers::on_request_user_input_response;

enum CommandExecutionApprovalPresentation {
    Network(ApiNetworkApprovalContext),
    Command(CommandExecutionCompletionItem),
}

struct CommandExecutionCompletionItem {
    command: String,
    cwd: PathBuf,
    command_actions: Vec<ApiParsedCommand>,
}

fn guardian_auto_approval_review_notification(
    conversation_id: &ThreadId,
    event_turn_id: &str,
    assessment: &GuardianAssessmentEvent,
) -> ServerNotification {
    // TODO(ccunningham): Attach guardian review state to the reviewed tool
    // item's lifecycle instead of sending standalone review notifications so
    // the app-gateway API can persist and replay review state via `thread/read`.
    let turn_id = if assessment.turn_id.is_empty() {
        event_turn_id.to_string()
    } else {
        assessment.turn_id.clone()
    };
    let review = GuardianApprovalReview {
        status: match assessment.status {
            praxis_protocol::protocol::GuardianAssessmentStatus::InProgress => {
                GuardianApprovalReviewStatus::InProgress
            }
            praxis_protocol::protocol::GuardianAssessmentStatus::Approved => {
                GuardianApprovalReviewStatus::Approved
            }
            praxis_protocol::protocol::GuardianAssessmentStatus::Denied => {
                GuardianApprovalReviewStatus::Denied
            }
            praxis_protocol::protocol::GuardianAssessmentStatus::Aborted => {
                GuardianApprovalReviewStatus::Aborted
            }
        },
        risk_score: assessment.risk_score,
        risk_level: assessment.risk_level.map(Into::into),
        rationale: assessment.rationale.clone(),
    };
    let action = assessment.action.clone().into();
    match assessment.status {
        praxis_protocol::protocol::GuardianAssessmentStatus::InProgress => {
            ServerNotification::ItemGuardianApprovalReviewStarted(
                ItemGuardianApprovalReviewStartedNotification {
                    thread_id: conversation_id.to_string(),
                    turn_id,
                    target_item_id: assessment.id.clone(),
                    review,
                    action,
                },
            )
        }
        praxis_protocol::protocol::GuardianAssessmentStatus::Approved
        | praxis_protocol::protocol::GuardianAssessmentStatus::Denied
        | praxis_protocol::protocol::GuardianAssessmentStatus::Aborted => {
            ServerNotification::ItemGuardianApprovalReviewCompleted(
                ItemGuardianApprovalReviewCompletedNotification {
                    thread_id: conversation_id.to_string(),
                    turn_id,
                    target_item_id: assessment.id.clone(),
                    review,
                    action,
                },
            )
        }
    }
}
