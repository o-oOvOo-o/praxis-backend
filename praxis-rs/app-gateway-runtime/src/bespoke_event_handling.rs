use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ClientRequestResult;
use crate::outgoing_message::ThreadScopedOutgoingMessageSender;
use crate::praxis_message_processor::read_rollout_items_from_rollout;
use crate::praxis_message_processor::read_summary_from_rollout;
use crate::praxis_message_processor::summary_to_thread;
use crate::server_request_error::is_turn_transition_server_request_error;
use crate::thread_state::ThreadListenerCommand;
use crate::thread_state::ThreadState;
use crate::thread_state::TurnSummary;
use crate::thread_status::ThreadWatchActiveGuard;
use crate::thread_status::ThreadWatchManager;
use praxis_app_gateway_protocol::AccountRateLimitsUpdatedNotification;
use praxis_app_gateway_protocol::AdditionalPermissionProfile as ApiAdditionalPermissionProfile;
use praxis_app_gateway_protocol::AgentMessageDeltaNotification;
use praxis_app_gateway_protocol::CodexErrorInfo as ApiCodexErrorInfo;
use praxis_app_gateway_protocol::CollabAgentState as ApiCollabAgentStatus;
use praxis_app_gateway_protocol::CollabAgentTool;
use praxis_app_gateway_protocol::CollabAgentToolCallStatus as ApiCollabToolCallStatus;
use praxis_app_gateway_protocol::CommandAction as ApiParsedCommand;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionOutputDeltaNotification;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalParams;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalResponse;
use praxis_app_gateway_protocol::CommandExecutionSource;
use praxis_app_gateway_protocol::CommandExecutionStatus;
use praxis_app_gateway_protocol::DeprecationNoticeNotification;
use praxis_app_gateway_protocol::DynamicToolCallOutputContentItem;
use praxis_app_gateway_protocol::DynamicToolCallParams;
use praxis_app_gateway_protocol::DynamicToolCallStatus;
use praxis_app_gateway_protocol::ErrorNotification;
use praxis_app_gateway_protocol::ExecPolicyAmendment as ApiExecPolicyAmendment;
use praxis_app_gateway_protocol::FileChangeApprovalDecision;
use praxis_app_gateway_protocol::FileChangeOutputDeltaNotification;
use praxis_app_gateway_protocol::FileChangeRequestApprovalParams;
use praxis_app_gateway_protocol::FileChangeRequestApprovalResponse;
use praxis_app_gateway_protocol::FileUpdateChange;
use praxis_app_gateway_protocol::GrantedPermissionProfile as ApiGrantedPermissionProfile;
use praxis_app_gateway_protocol::GuardianApprovalReview;
use praxis_app_gateway_protocol::GuardianApprovalReviewStatus;
use praxis_app_gateway_protocol::HookCompletedNotification;
use praxis_app_gateway_protocol::HookStartedNotification;
use praxis_app_gateway_protocol::ItemCompletedNotification;
use praxis_app_gateway_protocol::ItemGuardianApprovalReviewCompletedNotification;
use praxis_app_gateway_protocol::ItemGuardianApprovalReviewStartedNotification;
use praxis_app_gateway_protocol::ItemStartedNotification;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::McpServerElicitationAction;
use praxis_app_gateway_protocol::McpServerElicitationRequestParams;
use praxis_app_gateway_protocol::McpServerElicitationRequestResponse;
use praxis_app_gateway_protocol::McpServerStartupState;
use praxis_app_gateway_protocol::McpServerStatusUpdatedNotification;
use praxis_app_gateway_protocol::McpToolCallError;
use praxis_app_gateway_protocol::McpToolCallResult;
use praxis_app_gateway_protocol::McpToolCallStatus;
use praxis_app_gateway_protocol::ModelReroutedNotification;
use praxis_app_gateway_protocol::NetworkApprovalContext as ApiNetworkApprovalContext;
use praxis_app_gateway_protocol::NetworkPolicyAmendment as ApiNetworkPolicyAmendment;
use praxis_app_gateway_protocol::NetworkPolicyRuleAction as ApiNetworkPolicyRuleAction;
use praxis_app_gateway_protocol::PatchApplyStatus;
use praxis_app_gateway_protocol::PermissionsRequestApprovalParams;
use praxis_app_gateway_protocol::PermissionsRequestApprovalResponse;
use praxis_app_gateway_protocol::PlanDeltaNotification;
use praxis_app_gateway_protocol::RawResponseItemCompletedNotification;
use praxis_app_gateway_protocol::ReasoningSummaryPartAddedNotification;
use praxis_app_gateway_protocol::ReasoningSummaryTextDeltaNotification;
use praxis_app_gateway_protocol::ReasoningTextDeltaNotification;
use praxis_app_gateway_protocol::RequestId;
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
use praxis_app_gateway_protocol::ThreadRealtimeClosedNotification;
use praxis_app_gateway_protocol::ThreadRealtimeErrorNotification;
use praxis_app_gateway_protocol::ThreadRealtimeItemAddedNotification;
use praxis_app_gateway_protocol::ThreadRealtimeOutputAudioDeltaNotification;
use praxis_app_gateway_protocol::ThreadRealtimeStartedNotification;
use praxis_app_gateway_protocol::ThreadRealtimeTranscriptUpdatedNotification;
use praxis_app_gateway_protocol::ThreadRollbackResponse;
use praxis_app_gateway_protocol::ThreadTokenUsage;
use praxis_app_gateway_protocol::ThreadTokenUsageUpdatedNotification;
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
use praxis_app_gateway_protocol::build_turns_from_rollout_items;
use praxis_app_gateway_protocol::convert_patch_changes;
use praxis_core::PraxisThread;
use praxis_core::ThreadManager;
use praxis_core::review_format::REVIEW_FALLBACK_MESSAGE;
use praxis_core::review_format::render_review_output_text;
use praxis_core::review_prompts;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem as CoreDynamicToolCallOutputContentItem;
use praxis_protocol::dynamic_tools::DynamicToolResponse as CoreDynamicToolResponse;
use praxis_protocol::items::parse_hook_prompt_message;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::CodexErrorInfo as CoreCodexErrorInfo;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::GuardianAssessmentEvent;
use praxis_protocol::protocol::McpToolCallBeginEvent;
use praxis_protocol::protocol::McpToolCallEndEvent;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::RealtimeEvent;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::ReviewOutputEvent;
use praxis_protocol::protocol::TokenCountEvent;
use praxis_protocol::protocol::TurnDiffEvent;
use praxis_protocol::request_permissions::PermissionGrantScope as CorePermissionGrantScope;
use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsResponse as CoreRequestPermissionsResponse;
use praxis_protocol::request_user_input::RequestUserInputAnswer as CoreRequestUserInputAnswer;
use praxis_protocol::request_user_input::RequestUserInputResponse as CoreRequestUserInputResponse;
use praxis_sandboxing::policy_transforms::intersect_permission_profiles;
use praxis_shell_command::parse_command::shlex_join;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tracing::error;
use tracing::warn;

type JsonValue = serde_json::Value;

enum CommandExecutionApprovalPresentation {
    Network(ApiNetworkApprovalContext),
    Command(CommandExecutionCompletionItem),
}

struct CommandExecutionCompletionItem {
    command: String,
    cwd: PathBuf,
    command_actions: Vec<ApiParsedCommand>,
}

async fn resolve_server_request_on_thread_listener(
    thread_state: &Arc<Mutex<ThreadState>>,
    request_id: RequestId,
) {
    let (completion_tx, completion_rx) = oneshot::channel();
    let listener_command_tx = {
        let state = thread_state.lock().await;
        state.listener_command_tx()
    };
    let Some(listener_command_tx) = listener_command_tx else {
        error!("failed to remove pending client request: thread listener is not running");
        return;
    };

    if listener_command_tx
        .send(ThreadListenerCommand::ResolveServerRequest {
            request_id,
            completion_tx,
        })
        .is_err()
    {
        error!(
            "failed to remove pending client request: thread listener command channel is closed"
        );
        return;
    }

    if let Err(err) = completion_rx.await {
        error!("failed to remove pending client request: {err}");
    }
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

fn api_collab_interaction_tool(
    kind: praxis_protocol::protocol::CollabAgentInteractionKind,
) -> CollabAgentTool {
    match kind {
        praxis_protocol::protocol::CollabAgentInteractionKind::SendMessage => {
            CollabAgentTool::SendMessage
        }
        praxis_protocol::protocol::CollabAgentInteractionKind::AssignTask => {
            CollabAgentTool::AssignTask
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_bespoke_event_handling(
    event: Event,
    conversation_id: ThreadId,
    conversation: Arc<PraxisThread>,
    thread_manager: Arc<ThreadManager>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state: Arc<tokio::sync::Mutex<ThreadState>>,
    thread_watch_manager: ThreadWatchManager,
    fallback_model_provider: String,
    praxis_home: &Path,
) {
    let Event {
        id: event_turn_id,
        msg,
    } = event;
    match msg {
        EventMsg::TurnStarted(payload) => {
            // While not technically necessary as it was already done on TurnComplete, be extra cautios and abort any pending server requests.
            outgoing.abort_pending_server_requests().await;
            thread_watch_manager
                .note_turn_started(&conversation_id.to_string())
                .await;
            {
                let turn = {
                    let state = thread_state.lock().await;
                    state.active_turn_snapshot().unwrap_or_else(|| Turn {
                        id: payload.turn_id.clone(),
                        items: Vec::new(),
                        error: None,
                        status: TurnStatus::InProgress,
                    })
                };
                let notification = TurnStartedNotification {
                    thread_id: conversation_id.to_string(),
                    turn,
                };
                outgoing
                    .send_server_notification(ServerNotification::TurnStarted(notification))
                    .await;
            }
        }
        EventMsg::TurnComplete(_ev) => {
            // All per-thread requests are bound to a turn, so abort them.
            outgoing.abort_pending_server_requests().await;
            let turn_failed = thread_state.lock().await.turn_summary.last_error.is_some();
            thread_watch_manager
                .note_turn_completed(&conversation_id.to_string(), turn_failed)
                .await;
            handle_turn_complete(conversation_id, event_turn_id, &outgoing, &thread_state).await;
        }
        EventMsg::SkillsUpdateAvailable => {
            outgoing
                .send_server_notification(ServerNotification::SkillsChanged(
                    SkillsChangedNotification {},
                ))
                .await;
        }
        EventMsg::McpStartupUpdate(update) => {
            let (status, error) = match update.status {
                praxis_protocol::protocol::McpStartupStatus::Starting => {
                    (McpServerStartupState::Starting, None)
                }
                praxis_protocol::protocol::McpStartupStatus::Ready => {
                    (McpServerStartupState::Ready, None)
                }
                praxis_protocol::protocol::McpStartupStatus::Failed { error } => {
                    (McpServerStartupState::Failed, Some(error))
                }
                praxis_protocol::protocol::McpStartupStatus::Cancelled => {
                    (McpServerStartupState::Cancelled, None)
                }
            };
            let notification = McpServerStatusUpdatedNotification {
                name: update.server,
                status,
                error,
            };
            outgoing
                .send_server_notification(ServerNotification::McpServerStatusUpdated(notification))
                .await;
        }
        EventMsg::Warning(_warning_event) => {}
        EventMsg::GuardianAssessment(assessment) => {
            let notification = guardian_auto_approval_review_notification(
                &conversation_id,
                &event_turn_id,
                &assessment,
            );
            outgoing.send_server_notification(notification).await;
        }
        EventMsg::ModelReroute(event) => {
            let notification = ModelReroutedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                from_model: event.from_model,
                to_model: event.to_model,
                reason: event.reason.into(),
            };
            outgoing
                .send_server_notification(ServerNotification::ModelRerouted(notification))
                .await;
        }
        EventMsg::RealtimeConversationStarted(event) => {
            let notification = ThreadRealtimeStartedNotification {
                thread_id: conversation_id.to_string(),
                session_id: event.session_id,
                version: event.version,
            };
            outgoing
                .send_server_notification(ServerNotification::ThreadRealtimeStarted(notification))
                .await;
        }
        EventMsg::RealtimeConversationRealtime(event) => match event.payload {
            RealtimeEvent::SessionUpdated { .. } => {}
            RealtimeEvent::InputAudioSpeechStarted(event) => {
                let notification = ThreadRealtimeItemAddedNotification {
                    thread_id: conversation_id.to_string(),
                    item: serde_json::json!({
                        "type": "input_audio_buffer.speech_started",
                        "item_id": event.item_id,
                    }),
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeItemAdded(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::InputTranscriptDelta(event) => {
                let notification = ThreadRealtimeTranscriptUpdatedNotification {
                    thread_id: conversation_id.to_string(),
                    role: "user".to_string(),
                    text: event.delta,
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeTranscriptUpdated(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::OutputTranscriptDelta(event) => {
                let notification = ThreadRealtimeTranscriptUpdatedNotification {
                    thread_id: conversation_id.to_string(),
                    role: "assistant".to_string(),
                    text: event.delta,
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeTranscriptUpdated(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::AudioOut(audio) => {
                let notification = ThreadRealtimeOutputAudioDeltaNotification {
                    thread_id: conversation_id.to_string(),
                    audio: audio.into(),
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeOutputAudioDelta(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::ResponseCancelled(event) => {
                let notification = ThreadRealtimeItemAddedNotification {
                    thread_id: conversation_id.to_string(),
                    item: serde_json::json!({
                        "type": "response.cancelled",
                        "response_id": event.response_id,
                    }),
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeItemAdded(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::ConversationItemAdded(item) => {
                let notification = ThreadRealtimeItemAddedNotification {
                    thread_id: conversation_id.to_string(),
                    item,
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeItemAdded(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::ConversationItemDone { .. } => {}
            RealtimeEvent::HandoffRequested(handoff) => {
                let notification = ThreadRealtimeItemAddedNotification {
                    thread_id: conversation_id.to_string(),
                    item: serde_json::json!({
                        "type": "handoff_request",
                        "handoff_id": handoff.handoff_id,
                        "item_id": handoff.item_id,
                        "input_transcript": handoff.input_transcript,
                        "active_transcript": handoff.active_transcript,
                    }),
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeItemAdded(
                        notification,
                    ))
                    .await;
            }
            RealtimeEvent::Error(message) => {
                let notification = ThreadRealtimeErrorNotification {
                    thread_id: conversation_id.to_string(),
                    message,
                };
                outgoing
                    .send_server_notification(ServerNotification::ThreadRealtimeError(notification))
                    .await;
            }
        },
        EventMsg::RealtimeConversationClosed(event) => {
            let notification = ThreadRealtimeClosedNotification {
                thread_id: conversation_id.to_string(),
                reason: event.reason,
            };
            outgoing
                .send_server_notification(ServerNotification::ThreadRealtimeClosed(notification))
                .await;
        }
        EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id,
            turn_id,
            changes,
            reason,
            grant_root,
        }) => {
            let permission_guard = thread_watch_manager
                .note_permission_requested(&conversation_id.to_string())
                .await;
            // Until we migrate the core to be aware of a first class FileChangeItem
            // and emit the corresponding EventMsg, we repurpose the call_id as the item_id.
            let item_id = call_id.clone();
            let patch_changes = convert_patch_changes(&changes);

            let first_start = {
                let mut state = thread_state.lock().await;
                state
                    .turn_summary
                    .file_change_started
                    .insert(item_id.clone())
            };
            if first_start {
                let item = ThreadItem::FileChange {
                    id: item_id.clone(),
                    changes: patch_changes.clone(),
                    status: PatchApplyStatus::InProgress,
                };
                let notification = ItemStartedNotification {
                    thread_id: conversation_id.to_string(),
                    turn_id: event_turn_id.clone(),
                    item,
                };
                outgoing
                    .send_server_notification(ServerNotification::ItemStarted(notification))
                    .await;
            }

            let params = FileChangeRequestApprovalParams {
                thread_id: conversation_id.to_string(),
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                reason,
                grant_root,
            };
            let (pending_request_id, rx) = outgoing
                .send_request(ServerRequestPayload::FileChangeRequestApproval(params))
                .await;
            tokio::spawn(async move {
                on_file_change_request_approval_response(
                    event_turn_id,
                    conversation_id,
                    item_id,
                    patch_changes,
                    pending_request_id,
                    rx,
                    conversation,
                    outgoing,
                    thread_state.clone(),
                    permission_guard,
                )
                .await;
            });
        }
        EventMsg::ExecApprovalRequest(ev) => {
            let permission_guard = thread_watch_manager
                .note_permission_requested(&conversation_id.to_string())
                .await;
            let available_decisions = ev
                .effective_available_decisions()
                .into_iter()
                .map(CommandExecutionApprovalDecision::from)
                .collect::<Vec<_>>();
            let ExecApprovalRequestEvent {
                call_id,
                approval_id,
                turn_id,
                command,
                cwd,
                reason,
                network_approval_context,
                proposed_execpolicy_amendment,
                proposed_network_policy_amendments,
                additional_permissions,
                parsed_cmd,
                ..
            } = ev;
            let command_actions = parsed_cmd
                .iter()
                .cloned()
                .map(ApiParsedCommand::from)
                .collect::<Vec<_>>();
            let presentation = if let Some(network_approval_context) =
                network_approval_context.map(ApiNetworkApprovalContext::from)
            {
                CommandExecutionApprovalPresentation::Network(network_approval_context)
            } else {
                let command_string = shlex_join(&command);
                let completion_item = CommandExecutionCompletionItem {
                    command: command_string,
                    cwd: cwd.clone(),
                    command_actions: command_actions.clone(),
                };
                CommandExecutionApprovalPresentation::Command(completion_item)
            };
            let (network_approval_context, command, cwd, command_actions, completion_item) =
                match presentation {
                    CommandExecutionApprovalPresentation::Network(network_approval_context) => {
                        (Some(network_approval_context), None, None, None, None)
                    }
                    CommandExecutionApprovalPresentation::Command(completion_item) => (
                        None,
                        Some(completion_item.command.clone()),
                        Some(completion_item.cwd.clone()),
                        Some(completion_item.command_actions.clone()),
                        Some(completion_item),
                    ),
                };
            let proposed_execpolicy_amendment =
                proposed_execpolicy_amendment.map(ApiExecPolicyAmendment::from);
            let proposed_network_policy_amendments =
                proposed_network_policy_amendments.map(|amendments| {
                    amendments
                        .into_iter()
                        .map(ApiNetworkPolicyAmendment::from)
                        .collect()
                });
            let additional_permissions =
                additional_permissions.map(ApiAdditionalPermissionProfile::from);

            let params = CommandExecutionRequestApprovalParams {
                thread_id: conversation_id.to_string(),
                turn_id: turn_id.clone(),
                item_id: call_id.clone(),
                approval_id: approval_id.clone(),
                reason,
                network_approval_context,
                command,
                cwd,
                command_actions,
                additional_permissions,
                proposed_execpolicy_amendment,
                proposed_network_policy_amendments,
                available_decisions: Some(available_decisions),
            };
            let (pending_request_id, rx) = outgoing
                .send_request(ServerRequestPayload::CommandExecutionRequestApproval(
                    params,
                ))
                .await;
            tokio::spawn(async move {
                on_command_execution_request_approval_response(
                    event_turn_id,
                    conversation_id,
                    approval_id,
                    call_id,
                    completion_item,
                    pending_request_id,
                    rx,
                    conversation,
                    outgoing,
                    thread_state.clone(),
                    permission_guard,
                )
                .await;
            });
        }
        EventMsg::RequestUserInput(request) => {
            let user_input_guard = thread_watch_manager
                .note_user_input_requested(&conversation_id.to_string())
                .await;
            let questions = request
                .questions
                .into_iter()
                .map(|question| ToolRequestUserInputQuestion {
                    id: question.id,
                    header: question.header,
                    question: question.question,
                    is_other: question.is_other,
                    is_secret: question.is_secret,
                    options: question.options.map(|options| {
                        options
                            .into_iter()
                            .map(|option| ToolRequestUserInputOption {
                                label: option.label,
                                description: option.description,
                            })
                            .collect()
                    }),
                })
                .collect();
            let params = ToolRequestUserInputParams {
                thread_id: conversation_id.to_string(),
                turn_id: request.turn_id,
                item_id: request.call_id,
                questions,
            };
            let (pending_request_id, rx) = outgoing
                .send_request(ServerRequestPayload::ToolRequestUserInput(params))
                .await;
            tokio::spawn(async move {
                on_request_user_input_response(
                    event_turn_id,
                    pending_request_id,
                    rx,
                    conversation,
                    thread_state,
                    user_input_guard,
                )
                .await;
            });
        }
        EventMsg::ElicitationRequest(request) => {
            let permission_guard = thread_watch_manager
                .note_permission_requested(&conversation_id.to_string())
                .await;
            let turn_id = match request.turn_id.clone() {
                Some(turn_id) => Some(turn_id),
                None => {
                    let state = thread_state.lock().await;
                    state.active_turn_snapshot().map(|turn| turn.id)
                }
            };
            let server_name = request.server_name.clone();
            let request_body = match request.request.try_into() {
                Ok(request_body) => request_body,
                Err(err) => {
                    error!(
                        error = %err,
                        server_name,
                        request_id = ?request.id,
                        "failed to parse typed MCP elicitation schema"
                    );
                    if let Err(err) = conversation
                        .submit(Op::ResolveElicitation {
                            server_name: request.server_name,
                            request_id: request.id,
                            decision: praxis_protocol::approvals::ElicitationAction::Cancel,
                            content: None,
                            meta: None,
                        })
                        .await
                    {
                        error!("failed to submit ResolveElicitation: {err}");
                    }
                    return;
                }
            };
            let params = McpServerElicitationRequestParams {
                thread_id: conversation_id.to_string(),
                turn_id,
                server_name: request.server_name.clone(),
                request: request_body,
            };
            let (pending_request_id, rx) = outgoing
                .send_request(ServerRequestPayload::McpServerElicitationRequest(params))
                .await;
            tokio::spawn(async move {
                on_mcp_server_elicitation_response(
                    request.server_name,
                    request.id,
                    pending_request_id,
                    rx,
                    conversation,
                    thread_state,
                    permission_guard,
                )
                .await;
            });
        }
        EventMsg::RequestPermissions(request) => {
            let permission_guard = thread_watch_manager
                .note_permission_requested(&conversation_id.to_string())
                .await;
            let requested_permissions = request.permissions.clone();
            let params = PermissionsRequestApprovalParams {
                thread_id: conversation_id.to_string(),
                turn_id: request.turn_id.clone(),
                item_id: request.call_id.clone(),
                reason: request.reason,
                permissions: request.permissions.into(),
            };
            let (pending_request_id, rx) = outgoing
                .send_request(ServerRequestPayload::PermissionsRequestApproval(params))
                .await;
            tokio::spawn(async move {
                on_request_permissions_response(
                    request.call_id,
                    requested_permissions,
                    pending_request_id,
                    rx,
                    conversation,
                    thread_state,
                    permission_guard,
                )
                .await;
            });
        }
        EventMsg::DynamicToolCallRequest(request) => {
            let call_id = request.call_id;
            let turn_id = request.turn_id;
            let tool = request.tool;
            let arguments = request.arguments;
            let item = ThreadItem::DynamicToolCall {
                id: call_id.clone(),
                tool: tool.clone(),
                arguments: arguments.clone(),
                status: DynamicToolCallStatus::InProgress,
                content_items: None,
                success: None,
                duration_ms: None,
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
            let params = DynamicToolCallParams {
                thread_id: conversation_id.to_string(),
                turn_id: turn_id.clone(),
                call_id: call_id.clone(),
                tool: tool.clone(),
                arguments: arguments.clone(),
            };
            let (_pending_request_id, rx) = outgoing
                .send_request(ServerRequestPayload::DynamicToolCall(params))
                .await;
            tokio::spawn(async move {
                crate::dynamic_tools::on_call_response(call_id, rx, conversation).await;
            });
        }
        EventMsg::DynamicToolCallResponse(response) => {
            let status = if response.success {
                DynamicToolCallStatus::Completed
            } else {
                DynamicToolCallStatus::Failed
            };
            let duration_ms = i64::try_from(response.duration.as_millis()).ok();
            let item = ThreadItem::DynamicToolCall {
                id: response.call_id,
                tool: response.tool,
                arguments: response.arguments,
                status,
                content_items: Some(
                    response
                        .content_items
                        .into_iter()
                        .map(|item| match item {
                            CoreDynamicToolCallOutputContentItem::InputText { text } => {
                                DynamicToolCallOutputContentItem::InputText { text }
                            }
                            CoreDynamicToolCallOutputContentItem::InputImage { image_url } => {
                                DynamicToolCallOutputContentItem::InputImage { image_url }
                            }
                        })
                        .collect(),
                ),
                success: Some(response.success),
                duration_ms,
            };
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: response.turn_id,
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        // TODO(celia): properly construct McpToolCall TurnItem in core.
        EventMsg::McpToolCallBegin(begin_event) => {
            let notification = construct_mcp_tool_call_notification(
                begin_event,
                conversation_id.to_string(),
                event_turn_id.clone(),
            )
            .await;
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::McpToolCallEnd(end_event) => {
            let notification = construct_mcp_tool_call_end_notification(
                end_event,
                conversation_id.to_string(),
                event_turn_id.clone(),
            )
            .await;
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::CollabAgentSpawnBegin(begin_event) => {
            let item = ThreadItem::CollabAgentToolCall {
                id: begin_event.call_id,
                tool: CollabAgentTool::SpawnAgent,
                status: ApiCollabToolCallStatus::InProgress,
                sender_thread_id: begin_event.sender_thread_id.to_string(),
                receiver_thread_ids: Vec::new(),
                prompt: Some(begin_event.prompt),
                model: Some(begin_event.model),
                reasoning_effort: Some(begin_event.reasoning_effort),
                agents_states: HashMap::new(),
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::CollabAgentSpawnEnd(end_event) => {
            let has_receiver = end_event.new_thread_id.is_some();
            let status = match &end_event.status {
                praxis_protocol::protocol::AgentStatus::Errored(_)
                | praxis_protocol::protocol::AgentStatus::NotFound => {
                    ApiCollabToolCallStatus::Failed
                }
                _ if has_receiver => ApiCollabToolCallStatus::Completed,
                _ => ApiCollabToolCallStatus::Failed,
            };
            let (receiver_thread_ids, agents_states) = match end_event.new_thread_id {
                Some(id) => {
                    let receiver_id = id.to_string();
                    let received_status = ApiCollabAgentStatus::from(end_event.status.clone());
                    (
                        vec![receiver_id.clone()],
                        [(receiver_id, received_status)].into_iter().collect(),
                    )
                }
                None => (Vec::new(), HashMap::new()),
            };
            let item = ThreadItem::CollabAgentToolCall {
                id: end_event.call_id,
                tool: CollabAgentTool::SpawnAgent,
                status,
                sender_thread_id: end_event.sender_thread_id.to_string(),
                receiver_thread_ids,
                prompt: Some(end_event.prompt),
                model: Some(end_event.model),
                reasoning_effort: Some(end_event.reasoning_effort),
                agents_states,
            };
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::CollabAgentInteractionBegin(begin_event) => {
            if matches!(
                begin_event.kind,
                praxis_protocol::protocol::CollabAgentInteractionKind::AssignTask
            ) {
                thread_watch_manager
                    .acquire_thread_control(
                        &begin_event.receiver_thread_id.to_string(),
                        ThreadController {
                            kind: ThreadControllerKind::Thread,
                            id: begin_event.sender_thread_id.to_string(),
                            label: Some(format!(
                                "thread {}",
                                begin_event
                                    .sender_thread_id
                                    .to_string()
                                    .chars()
                                    .take(8)
                                    .collect::<String>()
                            )),
                            rank: Some(0),
                        },
                        Some(begin_event.prompt.clone()),
                    )
                    .await;
            }
            let receiver_thread_ids = vec![begin_event.receiver_thread_id.to_string()];
            let item = ThreadItem::CollabAgentToolCall {
                id: begin_event.call_id,
                tool: api_collab_interaction_tool(begin_event.kind),
                status: ApiCollabToolCallStatus::InProgress,
                sender_thread_id: begin_event.sender_thread_id.to_string(),
                receiver_thread_ids,
                prompt: Some(begin_event.prompt),
                model: None,
                reasoning_effort: None,
                agents_states: HashMap::new(),
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::CollabAgentInteractionEnd(end_event) => {
            let status = match &end_event.status {
                praxis_protocol::protocol::AgentStatus::Errored(_)
                | praxis_protocol::protocol::AgentStatus::NotFound => {
                    ApiCollabToolCallStatus::Failed
                }
                _ => ApiCollabToolCallStatus::Completed,
            };
            if matches!(&status, ApiCollabToolCallStatus::Failed) {
                thread_watch_manager
                    .release_thread_control(&end_event.receiver_thread_id.to_string())
                    .await;
            }
            let receiver_id = end_event.receiver_thread_id.to_string();
            let received_status = ApiCollabAgentStatus::from(end_event.status);
            let item = ThreadItem::CollabAgentToolCall {
                id: end_event.call_id,
                tool: api_collab_interaction_tool(end_event.kind),
                status,
                sender_thread_id: end_event.sender_thread_id.to_string(),
                receiver_thread_ids: vec![receiver_id.clone()],
                prompt: Some(end_event.prompt),
                model: None,
                reasoning_effort: None,
                agents_states: [(receiver_id, received_status)].into_iter().collect(),
            };
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::CollabWaitingBegin(begin_event) => {
            let receiver_thread_ids = begin_event
                .receiver_thread_ids
                .iter()
                .map(ToString::to_string)
                .collect();
            let item = ThreadItem::CollabAgentToolCall {
                id: begin_event.call_id,
                tool: CollabAgentTool::Wait,
                status: ApiCollabToolCallStatus::InProgress,
                sender_thread_id: begin_event.sender_thread_id.to_string(),
                receiver_thread_ids,
                prompt: None,
                model: None,
                reasoning_effort: None,
                agents_states: HashMap::new(),
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::CollabWaitingEnd(end_event) => {
            let status = if end_event.statuses.values().any(|status| {
                matches!(
                    status,
                    praxis_protocol::protocol::AgentStatus::Errored(_)
                        | praxis_protocol::protocol::AgentStatus::NotFound
                )
            }) {
                ApiCollabToolCallStatus::Failed
            } else {
                ApiCollabToolCallStatus::Completed
            };
            let receiver_thread_ids = end_event.statuses.keys().map(ToString::to_string).collect();
            let agents_states = end_event
                .statuses
                .iter()
                .map(|(id, status)| (id.to_string(), ApiCollabAgentStatus::from(status.clone())))
                .collect();
            let item = ThreadItem::CollabAgentToolCall {
                id: end_event.call_id,
                tool: CollabAgentTool::Wait,
                status,
                sender_thread_id: end_event.sender_thread_id.to_string(),
                receiver_thread_ids,
                prompt: None,
                model: None,
                reasoning_effort: None,
                agents_states,
            };
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::CollabCloseBegin(begin_event) => {
            let item = ThreadItem::CollabAgentToolCall {
                id: begin_event.call_id,
                tool: CollabAgentTool::CloseAgent,
                status: ApiCollabToolCallStatus::InProgress,
                sender_thread_id: begin_event.sender_thread_id.to_string(),
                receiver_thread_ids: vec![begin_event.receiver_thread_id.to_string()],
                prompt: None,
                model: None,
                reasoning_effort: None,
                agents_states: HashMap::new(),
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::CollabCloseEnd(end_event) => {
            if thread_manager
                .get_thread(end_event.receiver_thread_id)
                .await
                .is_err()
            {
                thread_watch_manager
                    .remove_thread(&end_event.receiver_thread_id.to_string())
                    .await;
                outgoing
                    .send_server_notification(ServerNotification::ThreadClosed(
                        ThreadClosedNotification {
                            thread_id: end_event.receiver_thread_id.to_string(),
                        },
                    ))
                    .await;
            }
            let status = match &end_event.status {
                praxis_protocol::protocol::AgentStatus::Errored(_)
                | praxis_protocol::protocol::AgentStatus::NotFound => {
                    ApiCollabToolCallStatus::Failed
                }
                _ => ApiCollabToolCallStatus::Completed,
            };
            let receiver_id = end_event.receiver_thread_id.to_string();
            let agents_states = [(
                receiver_id.clone(),
                ApiCollabAgentStatus::from(end_event.status),
            )]
            .into_iter()
            .collect();
            let item = ThreadItem::CollabAgentToolCall {
                id: end_event.call_id,
                tool: CollabAgentTool::CloseAgent,
                status,
                sender_thread_id: end_event.sender_thread_id.to_string(),
                receiver_thread_ids: vec![receiver_id],
                prompt: None,
                model: None,
                reasoning_effort: None,
                agents_states,
            };
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::CollabResumeBegin(begin_event) => {
            let item = collab_resume_begin_item(begin_event);
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::CollabResumeEnd(end_event) => {
            let item = collab_resume_end_item(end_event);
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::AgentMessageContentDelta(event) => {
            let praxis_protocol::protocol::AgentMessageContentDeltaEvent { item_id, delta, .. } =
                event;
            let notification = AgentMessageDeltaNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item_id,
                delta,
            };
            outgoing
                .send_server_notification(ServerNotification::AgentMessageDelta(notification))
                .await;
        }
        EventMsg::PlanDelta(event) => {
            let notification = PlanDeltaNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item_id: event.item_id,
                delta: event.delta,
            };
            outgoing
                .send_server_notification(ServerNotification::PlanDelta(notification))
                .await;
        }
        EventMsg::ContextCompacted(..) => {}
        EventMsg::DeprecationNotice(event) => {
            let notification = DeprecationNoticeNotification {
                summary: event.summary,
                details: event.details,
            };
            outgoing
                .send_server_notification(ServerNotification::DeprecationNotice(notification))
                .await;
        }
        EventMsg::ReasoningContentDelta(event) => {
            let notification = ReasoningSummaryTextDeltaNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item_id: event.item_id,
                delta: event.delta,
                summary_index: event.summary_index,
            };
            outgoing
                .send_server_notification(ServerNotification::ReasoningSummaryTextDelta(
                    notification,
                ))
                .await;
        }
        EventMsg::ReasoningRawContentDelta(event) => {
            let notification = ReasoningTextDeltaNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item_id: event.item_id,
                delta: event.delta,
                content_index: event.content_index,
            };
            outgoing
                .send_server_notification(ServerNotification::ReasoningTextDelta(notification))
                .await;
        }
        EventMsg::AgentReasoningSectionBreak(event) => {
            let notification = ReasoningSummaryPartAddedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item_id: event.item_id,
                summary_index: event.summary_index,
            };
            outgoing
                .send_server_notification(ServerNotification::ReasoningSummaryPartAdded(
                    notification,
                ))
                .await;
        }
        EventMsg::TokenCount(token_count_event) => {
            handle_token_count_event(conversation_id, event_turn_id, token_count_event, &outgoing)
                .await;
        }
        EventMsg::Error(ev) => {
            thread_watch_manager
                .note_system_error(&conversation_id.to_string())
                .await;

            let message = ev.message.clone();
            let praxis_error_info = ev.praxis_error_info.clone();

            // If this error belongs to an in-flight `thread/rollback` request, fail that request
            // (and clear pending state) so subsequent rollbacks are unblocked.
            //
            // Don't send a notification for this error.
            if matches!(
                praxis_error_info,
                Some(CoreCodexErrorInfo::ThreadRollbackFailed)
            ) {
                return handle_thread_rollback_failed(
                    conversation_id,
                    message,
                    &thread_state,
                    &outgoing,
                )
                .await;
            };

            if !ev.affects_turn_status() {
                return;
            }

            let turn_error = TurnError {
                message: ev.message,
                praxis_error_info: ev.praxis_error_info.map(ApiCodexErrorInfo::from),
                additional_details: None,
            };
            handle_error(conversation_id, turn_error.clone(), &thread_state).await;
            outgoing
                .send_server_notification(ServerNotification::Error(ErrorNotification {
                    error: turn_error.clone(),
                    will_retry: false,
                    thread_id: conversation_id.to_string(),
                    turn_id: event_turn_id.clone(),
                }))
                .await;
        }
        EventMsg::StreamError(ev) => {
            // We don't need to update the turn summary store for stream errors as they are intermediate error states for retries,
            // but we notify the client.
            let turn_error = TurnError {
                message: ev.message,
                praxis_error_info: ev.praxis_error_info.map(ApiCodexErrorInfo::from),
                additional_details: ev.additional_details,
            };
            outgoing
                .send_server_notification(ServerNotification::Error(ErrorNotification {
                    error: turn_error,
                    will_retry: true,
                    thread_id: conversation_id.to_string(),
                    turn_id: event_turn_id.clone(),
                }))
                .await;
        }
        EventMsg::ViewImageToolCall(view_image_event) => {
            let item = ThreadItem::ImageView {
                id: view_image_event.call_id.clone(),
                path: view_image_event.path.to_string_lossy().into_owned(),
            };
            let started = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item: item.clone(),
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(started))
                .await;
            let completed = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(completed))
                .await;
        }
        EventMsg::WebSearchBegin(web_search_event) => {
            let item = ThreadItem::WebSearch {
                id: web_search_event.call_id,
                query: String::new(),
                action: None,
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::WebSearchEnd(web_search_event) => {
            let item = ThreadItem::WebSearch {
                id: web_search_event.call_id,
                query: web_search_event.query,
                action: Some(praxis_app_gateway_protocol::WebSearchAction::from(
                    web_search_event.action,
                )),
            };
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::EnteredReviewMode(review_request) => {
            let review = review_request
                .user_facing_hint
                .unwrap_or_else(|| review_prompts::user_facing_hint(&review_request.target));
            let item = ThreadItem::EnteredReviewMode {
                id: event_turn_id.clone(),
                review,
            };
            let started = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item: item.clone(),
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(started))
                .await;
            let completed = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(completed))
                .await;
        }
        EventMsg::ItemStarted(item_started_event) => {
            let item: ThreadItem = item_started_event.item.clone().into();
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::ItemCompleted(item_completed_event) => {
            let item: ThreadItem = item_completed_event.item.clone().into();
            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        EventMsg::HookStarted(event) => {
            let notification = HookStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event.turn_id,
                run: event.run.into(),
            };
            outgoing
                .send_server_notification(ServerNotification::HookStarted(notification))
                .await;
        }
        EventMsg::HookCompleted(event) => {
            let notification = HookCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event.turn_id,
                run: event.run.into(),
            };
            outgoing
                .send_server_notification(ServerNotification::HookCompleted(notification))
                .await;
        }
        EventMsg::ExitedReviewMode(review_event) => {
            let review = match review_event.review_output {
                Some(output) => render_review_output_text(&output),
                None => REVIEW_FALLBACK_MESSAGE.to_string(),
            };
            let item = ThreadItem::ExitedReviewMode {
                id: event_turn_id.clone(),
                review,
            };
            let started = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item: item.clone(),
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(started))
                .await;
            let completed = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(completed))
                .await;
        }
        EventMsg::RawResponseItem(raw_response_item_event) => {
            maybe_emit_hook_prompt_item_completed(
                conversation_id,
                &event_turn_id,
                &raw_response_item_event.item,
                &outgoing,
            )
            .await;
            maybe_emit_raw_response_item_completed(
                conversation_id,
                &event_turn_id,
                raw_response_item_event.item,
                &outgoing,
            )
            .await;
        }
        EventMsg::PatchApplyBegin(patch_begin_event) => {
            // Until we migrate the core to be aware of a first class FileChangeItem
            // and emit the corresponding EventMsg, we repurpose the call_id as the item_id.
            let item_id = patch_begin_event.call_id.clone();
            let changes = convert_patch_changes(&patch_begin_event.changes);

            let first_start = {
                let mut state = thread_state.lock().await;
                state
                    .turn_summary
                    .file_change_started
                    .insert(item_id.clone())
            };
            if first_start {
                let item = ThreadItem::FileChange {
                    id: item_id.clone(),
                    changes,
                    status: PatchApplyStatus::InProgress,
                };
                let notification = ItemStartedNotification {
                    thread_id: conversation_id.to_string(),
                    turn_id: event_turn_id.clone(),
                    item,
                };
                outgoing
                    .send_server_notification(ServerNotification::ItemStarted(notification))
                    .await;
            }
        }
        EventMsg::PatchApplyEnd(patch_end_event) => {
            // Until we migrate the core to be aware of a first class FileChangeItem
            // and emit the corresponding EventMsg, we repurpose the call_id as the item_id.
            let item_id = patch_end_event.call_id.clone();

            let status: PatchApplyStatus = (&patch_end_event.status).into();
            let changes = convert_patch_changes(&patch_end_event.changes);
            complete_file_change_item(
                conversation_id,
                item_id,
                changes,
                status,
                event_turn_id.clone(),
                &outgoing,
                &thread_state,
            )
            .await;
        }
        EventMsg::ExecCommandBegin(exec_command_begin_event) => {
            let item_id = exec_command_begin_event.call_id.clone();
            let command_actions = exec_command_begin_event
                .parsed_cmd
                .into_iter()
                .map(ApiParsedCommand::from)
                .collect::<Vec<_>>();
            let command = shlex_join(&exec_command_begin_event.command);
            let cwd = exec_command_begin_event.cwd;
            let process_id = exec_command_begin_event.process_id;

            {
                let mut state = thread_state.lock().await;
                state
                    .turn_summary
                    .command_execution_started
                    .insert(item_id.clone());
            }

            let item = ThreadItem::CommandExecution {
                id: item_id,
                command,
                cwd,
                process_id,
                source: exec_command_begin_event.source.into(),
                status: CommandExecutionStatus::InProgress,
                command_actions,
                aggregated_output: None,
                exit_code: None,
                duration_ms: None,
            };
            let notification = ItemStartedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemStarted(notification))
                .await;
        }
        EventMsg::ExecCommandOutputDelta(exec_command_output_delta_event) => {
            let item_id = exec_command_output_delta_event.call_id.clone();
            // The underlying EventMsg::ExecCommandOutputDelta is used for shell, unified_exec,
            // and apply_patch tool calls. We represent apply_patch with the FileChange item, and
            // everything else with the CommandExecution item.
            //
            // We need to detect which item type it is so we can emit the right notification.
            // We already have state tracking FileChange items on item/started, so let's use that.
            let is_file_change = {
                let state = thread_state.lock().await;
                state.turn_summary.file_change_started.contains(&item_id)
            };
            if is_file_change {
                let delta =
                    String::from_utf8_lossy(&exec_command_output_delta_event.chunk).to_string();
                let notification = FileChangeOutputDeltaNotification {
                    thread_id: conversation_id.to_string(),
                    turn_id: event_turn_id.clone(),
                    item_id,
                    delta,
                };
                outgoing
                    .send_server_notification(ServerNotification::FileChangeOutputDelta(
                        notification,
                    ))
                    .await;
            } else {
                let notification = CommandExecutionOutputDeltaNotification {
                    thread_id: conversation_id.to_string(),
                    turn_id: event_turn_id.clone(),
                    item_id,
                    delta: String::from_utf8_lossy(&exec_command_output_delta_event.chunk)
                        .to_string(),
                };
                outgoing
                    .send_server_notification(ServerNotification::CommandExecutionOutputDelta(
                        notification,
                    ))
                    .await;
            }
        }
        EventMsg::TerminalInteraction(terminal_event) => {
            let item_id = terminal_event.call_id.clone();

            let notification = TerminalInteractionNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item_id,
                process_id: terminal_event.process_id,
                stdin: terminal_event.stdin,
            };
            outgoing
                .send_server_notification(ServerNotification::TerminalInteraction(notification))
                .await;
        }
        EventMsg::ExecCommandEnd(exec_command_end_event) => {
            let ExecCommandEndEvent {
                call_id,
                command,
                cwd,
                parsed_cmd,
                process_id,
                aggregated_output,
                exit_code,
                duration,
                source,
                status,
                ..
            } = exec_command_end_event;

            {
                let mut state = thread_state.lock().await;
                state
                    .turn_summary
                    .command_execution_started
                    .remove(&call_id);
            }

            let status: CommandExecutionStatus = (&status).into();
            let command_actions = parsed_cmd
                .into_iter()
                .map(ApiParsedCommand::from)
                .collect::<Vec<_>>();

            let aggregated_output = if aggregated_output.is_empty() {
                None
            } else {
                Some(aggregated_output)
            };

            let duration_ms = i64::try_from(duration.as_millis()).unwrap_or(i64::MAX);

            let item = ThreadItem::CommandExecution {
                id: call_id,
                command: shlex_join(&command),
                cwd,
                process_id,
                source: source.into(),
                status,
                command_actions,
                aggregated_output,
                exit_code: Some(exit_code),
                duration_ms: Some(duration_ms),
            };

            let notification = ItemCompletedNotification {
                thread_id: conversation_id.to_string(),
                turn_id: event_turn_id.clone(),
                item,
            };
            outgoing
                .send_server_notification(ServerNotification::ItemCompleted(notification))
                .await;
        }
        // If this is a TurnAborted, reply to any pending interrupt requests.
        EventMsg::TurnAborted(turn_aborted_event) => {
            // All per-thread requests are bound to a turn, so abort them.
            outgoing.abort_pending_server_requests().await;
            let pending = {
                let mut state = thread_state.lock().await;
                std::mem::take(&mut state.pending_interrupts)
            };
            if !pending.is_empty() {
                for rid in pending {
                    outgoing.send_response(rid, TurnInterruptResponse {}).await;
                }
            }

            thread_watch_manager
                .note_turn_interrupted(&conversation_id.to_string())
                .await;
            handle_turn_interrupted(conversation_id, event_turn_id, &outgoing, &thread_state).await;
        }
        EventMsg::ThreadRolledBack(_rollback_event) => {
            let pending = {
                let mut state = thread_state.lock().await;
                state.pending_rollbacks.take()
            };

            if let Some(request_id) = pending {
                let Some(rollout_path) = conversation.rollout_path() else {
                    let error = JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "thread has no persisted rollout".to_string(),
                        data: None,
                    };
                    outgoing.send_error(request_id, error).await;
                    return;
                };
                let response = match read_summary_from_rollout(
                    rollout_path.as_path(),
                    fallback_model_provider.as_str(),
                )
                .await
                {
                    Ok(summary) => {
                        let mut thread = summary_to_thread(summary);
                        match read_rollout_items_from_rollout(rollout_path.as_path()).await {
                            Ok(items) => {
                                thread.turns = build_turns_from_rollout_items(&items);
                                thread.status = thread_watch_manager
                                    .loaded_status_for_thread(&thread.id)
                                    .await;
                                let state_db =
                                    praxis_rollout::state_db::open_if_present(praxis_home, "")
                                        .await;
                                thread.name =
                                    praxis_rollout::ThreadNameResolver::new(state_db.as_deref())
                                        .resolve_name(conversation_id)
                                        .await;
                                ThreadRollbackResponse { thread }
                            }
                            Err(err) => {
                                let error = JSONRPCErrorError {
                                    code: INTERNAL_ERROR_CODE,
                                    message: format!(
                                        "failed to load rollout `{}`: {err}",
                                        rollout_path.display()
                                    ),
                                    data: None,
                                };
                                outgoing.send_error(request_id.clone(), error).await;
                                return;
                            }
                        }
                    }
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!(
                                "failed to load rollout `{}`: {err}",
                                rollout_path.display()
                            ),
                            data: None,
                        };
                        outgoing.send_error(request_id.clone(), error).await;
                        return;
                    }
                };

                outgoing.send_response(request_id, response).await;
            }
        }
        EventMsg::ThreadNameUpdated(thread_name_event) => {
            let notification = ThreadNameUpdatedNotification {
                thread_id: thread_name_event.thread_id.to_string(),
                thread_name: thread_name_event.thread_name,
            };
            outgoing
                .send_global_server_notification(ServerNotification::ThreadNameUpdated(
                    notification,
                ))
                .await;
        }
        EventMsg::ThreadGoalUpdated(thread_goal_event) => {
            let notification = ThreadGoalUpdatedNotification {
                thread_id: thread_goal_event.thread_id.to_string(),
                turn_id: thread_goal_event.turn_id,
                goal: thread_goal_event.goal.into(),
            };
            outgoing
                .send_server_notification(ServerNotification::ThreadGoalUpdated(notification))
                .await;
        }
        EventMsg::TurnDiff(turn_diff_event) => {
            handle_turn_diff(conversation_id, &event_turn_id, turn_diff_event, &outgoing).await;
        }
        EventMsg::PlanUpdate(plan_update_event) => {
            handle_turn_plan_update(
                conversation_id,
                &event_turn_id,
                plan_update_event,
                &outgoing,
            )
            .await;
        }
        EventMsg::ShutdownComplete => {
            thread_watch_manager
                .note_thread_shutdown(&conversation_id.to_string())
                .await;
        }

        _ => {}
    }
}

async fn handle_turn_diff(
    conversation_id: ThreadId,
    event_turn_id: &str,
    turn_diff_event: TurnDiffEvent,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    {
        let notification = TurnDiffUpdatedNotification {
            thread_id: conversation_id.to_string(),
            turn_id: event_turn_id.to_string(),
            diff: turn_diff_event.unified_diff,
        };
        outgoing
            .send_server_notification(ServerNotification::TurnDiffUpdated(notification))
            .await;
    }
}

async fn handle_turn_plan_update(
    conversation_id: ThreadId,
    event_turn_id: &str,
    plan_update_event: UpdatePlanArgs,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    // `update_plan` is a todo/checklist tool; it is not related to plan-mode updates
    {
        let notification = TurnPlanUpdatedNotification {
            thread_id: conversation_id.to_string(),
            turn_id: event_turn_id.to_string(),
            explanation: plan_update_event.explanation,
            plan: plan_update_event
                .plan
                .into_iter()
                .map(TurnPlanStep::from)
                .collect(),
        };
        outgoing
            .send_server_notification(ServerNotification::TurnPlanUpdated(notification))
            .await;
    }
}

async fn emit_turn_completed_with_status(
    conversation_id: ThreadId,
    event_turn_id: String,
    status: TurnStatus,
    error: Option<TurnError>,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let notification = TurnCompletedNotification {
        thread_id: conversation_id.to_string(),
        turn: Turn {
            id: event_turn_id,
            items: vec![],
            error,
            status,
        },
    };
    outgoing
        .send_server_notification(ServerNotification::TurnCompleted(notification))
        .await;
}

async fn complete_file_change_item(
    conversation_id: ThreadId,
    item_id: String,
    changes: Vec<FileUpdateChange>,
    status: PatchApplyStatus,
    turn_id: String,
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_state: &Arc<Mutex<ThreadState>>,
) {
    let mut state = thread_state.lock().await;
    state.turn_summary.file_change_started.remove(&item_id);
    drop(state);

    let item = ThreadItem::FileChange {
        id: item_id,
        changes,
        status,
    };
    let notification = ItemCompletedNotification {
        thread_id: conversation_id.to_string(),
        turn_id,
        item,
    };
    outgoing
        .send_server_notification(ServerNotification::ItemCompleted(notification))
        .await;
}

#[allow(clippy::too_many_arguments)]
async fn complete_command_execution_item(
    conversation_id: ThreadId,
    turn_id: String,
    item_id: String,
    command: String,
    cwd: PathBuf,
    process_id: Option<String>,
    source: CommandExecutionSource,
    command_actions: Vec<ApiParsedCommand>,
    status: CommandExecutionStatus,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let item = ThreadItem::CommandExecution {
        id: item_id,
        command,
        cwd,
        process_id,
        source,
        status,
        command_actions,
        aggregated_output: None,
        exit_code: None,
        duration_ms: None,
    };
    let notification = ItemCompletedNotification {
        thread_id: conversation_id.to_string(),
        turn_id,
        item,
    };
    outgoing
        .send_server_notification(ServerNotification::ItemCompleted(notification))
        .await;
}

async fn maybe_emit_raw_response_item_completed(
    conversation_id: ThreadId,
    turn_id: &str,
    item: praxis_protocol::models::ResponseItem,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let notification = RawResponseItemCompletedNotification {
        thread_id: conversation_id.to_string(),
        turn_id: turn_id.to_string(),
        item,
    };
    outgoing
        .send_server_notification(ServerNotification::RawResponseItemCompleted(notification))
        .await;
}

async fn maybe_emit_hook_prompt_item_completed(
    conversation_id: ThreadId,
    turn_id: &str,
    item: &praxis_protocol::models::ResponseItem,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let praxis_protocol::models::ResponseItem::Message {
        role, content, id, ..
    } = item
    else {
        return;
    };

    if role != "user" {
        return;
    }

    let Some(hook_prompt) = parse_hook_prompt_message(id.as_ref(), content) else {
        return;
    };

    let notification = ItemCompletedNotification {
        thread_id: conversation_id.to_string(),
        turn_id: turn_id.to_string(),
        item: ThreadItem::HookPrompt {
            id: hook_prompt.id,
            fragments: hook_prompt
                .fragments
                .into_iter()
                .map(praxis_app_gateway_protocol::HookPromptFragment::from)
                .collect(),
        },
    };
    outgoing
        .send_server_notification(ServerNotification::ItemCompleted(notification))
        .await;
}

async fn find_and_remove_turn_summary(
    _conversation_id: ThreadId,
    thread_state: &Arc<Mutex<ThreadState>>,
) -> TurnSummary {
    let mut state = thread_state.lock().await;
    std::mem::take(&mut state.turn_summary)
}

async fn handle_turn_complete(
    conversation_id: ThreadId,
    event_turn_id: String,
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_state: &Arc<Mutex<ThreadState>>,
) {
    let turn_summary = find_and_remove_turn_summary(conversation_id, thread_state).await;

    let (status, error) = match turn_summary.last_error {
        Some(error) => (TurnStatus::Failed, Some(error)),
        None => (TurnStatus::Completed, None),
    };

    emit_turn_completed_with_status(conversation_id, event_turn_id, status, error, outgoing).await;
}

async fn handle_turn_interrupted(
    conversation_id: ThreadId,
    event_turn_id: String,
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_state: &Arc<Mutex<ThreadState>>,
) {
    find_and_remove_turn_summary(conversation_id, thread_state).await;

    emit_turn_completed_with_status(
        conversation_id,
        event_turn_id,
        TurnStatus::Interrupted,
        /*error*/ None,
        outgoing,
    )
    .await;
}

async fn handle_thread_rollback_failed(
    _conversation_id: ThreadId,
    message: String,
    thread_state: &Arc<Mutex<ThreadState>>,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let pending_rollback = thread_state.lock().await.pending_rollbacks.take();

    if let Some(request_id) = pending_rollback {
        outgoing
            .send_error(
                request_id,
                JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: message.clone(),
                    data: None,
                },
            )
            .await;
    }
}

async fn handle_token_count_event(
    conversation_id: ThreadId,
    turn_id: String,
    token_count_event: TokenCountEvent,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let TokenCountEvent { info, rate_limits } = token_count_event;
    if let Some(token_usage) = info.map(ThreadTokenUsage::from) {
        let notification = ThreadTokenUsageUpdatedNotification {
            thread_id: conversation_id.to_string(),
            turn_id,
            token_usage,
        };
        outgoing
            .send_server_notification(ServerNotification::ThreadTokenUsageUpdated(notification))
            .await;
    }
    if let Some(rate_limits) = rate_limits {
        outgoing
            .send_server_notification(ServerNotification::AccountRateLimitsUpdated(
                AccountRateLimitsUpdatedNotification {
                    rate_limits: rate_limits.into(),
                },
            ))
            .await;
    }
}

async fn handle_error(
    _conversation_id: ThreadId,
    error: TurnError,
    thread_state: &Arc<Mutex<ThreadState>>,
) {
    let mut state = thread_state.lock().await;
    state.turn_summary.last_error = Some(error);
}

async fn on_request_user_input_response(
    event_turn_id: String,
    pending_request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
    user_input_guard: ThreadWatchActiveGuard,
) {
    let response = receiver.await;
    resolve_server_request_on_thread_listener(&thread_state, pending_request_id).await;
    drop(user_input_guard);
    let value = match response {
        Ok(Ok(value)) => value,
        Ok(Err(err)) if is_turn_transition_server_request_error(&err) => return,
        Ok(Err(err)) => {
            error!("request failed with client error: {err:?}");
            let empty = CoreRequestUserInputResponse {
                answers: HashMap::new(),
            };
            if let Err(err) = conversation
                .submit(Op::UserInputAnswer {
                    id: event_turn_id,
                    response: empty,
                })
                .await
            {
                error!("failed to submit UserInputAnswer: {err}");
            }
            return;
        }
        Err(err) => {
            error!("request failed: {err:?}");
            let empty = CoreRequestUserInputResponse {
                answers: HashMap::new(),
            };
            if let Err(err) = conversation
                .submit(Op::UserInputAnswer {
                    id: event_turn_id,
                    response: empty,
                })
                .await
            {
                error!("failed to submit UserInputAnswer: {err}");
            }
            return;
        }
    };

    let response =
        serde_json::from_value::<ToolRequestUserInputResponse>(value).unwrap_or_else(|err| {
            error!("failed to deserialize ToolRequestUserInputResponse: {err}");
            ToolRequestUserInputResponse {
                answers: HashMap::new(),
            }
        });
    let response = CoreRequestUserInputResponse {
        answers: response
            .answers
            .into_iter()
            .map(|(id, answer)| {
                (
                    id,
                    CoreRequestUserInputAnswer {
                        answers: answer.answers,
                    },
                )
            })
            .collect(),
    };

    if let Err(err) = conversation
        .submit(Op::UserInputAnswer {
            id: event_turn_id,
            response,
        })
        .await
    {
        error!("failed to submit UserInputAnswer: {err}");
    }
}

async fn on_mcp_server_elicitation_response(
    server_name: String,
    request_id: praxis_protocol::mcp::RequestId,
    pending_request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
    permission_guard: ThreadWatchActiveGuard,
) {
    let response = receiver.await;
    resolve_server_request_on_thread_listener(&thread_state, pending_request_id).await;
    drop(permission_guard);
    let response = mcp_server_elicitation_response_from_client_result(response);

    if let Err(err) = conversation
        .submit(Op::ResolveElicitation {
            server_name,
            request_id,
            decision: response.action.to_core(),
            content: response.content,
            meta: response.meta,
        })
        .await
    {
        error!("failed to submit ResolveElicitation: {err}");
    }
}

fn mcp_server_elicitation_response_from_client_result(
    response: std::result::Result<ClientRequestResult, oneshot::error::RecvError>,
) -> McpServerElicitationRequestResponse {
    match response {
        Ok(Ok(value)) => serde_json::from_value::<McpServerElicitationRequestResponse>(value)
            .unwrap_or_else(|err| {
                error!("failed to deserialize McpServerElicitationRequestResponse: {err}");
                McpServerElicitationRequestResponse {
                    action: McpServerElicitationAction::Decline,
                    content: None,
                    meta: None,
                }
            }),
        Ok(Err(err)) if is_turn_transition_server_request_error(&err) => {
            McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Cancel,
                content: None,
                meta: None,
            }
        }
        Ok(Err(err)) => {
            error!("request failed with client error: {err:?}");
            McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Decline,
                content: None,
                meta: None,
            }
        }
        Err(err) => {
            error!("request failed: {err:?}");
            McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Decline,
                content: None,
                meta: None,
            }
        }
    }
}

async fn on_request_permissions_response(
    call_id: String,
    requested_permissions: CoreRequestPermissionProfile,
    pending_request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
    request_permissions_guard: ThreadWatchActiveGuard,
) {
    let response = receiver.await;
    resolve_server_request_on_thread_listener(&thread_state, pending_request_id).await;
    drop(request_permissions_guard);
    let Some(response) =
        request_permissions_response_from_client_result(requested_permissions, response)
    else {
        return;
    };

    if let Err(err) = conversation
        .submit(Op::RequestPermissionsResponse {
            id: call_id,
            response,
        })
        .await
    {
        error!("failed to submit RequestPermissionsResponse: {err}");
    }
}

fn request_permissions_response_from_client_result(
    requested_permissions: CoreRequestPermissionProfile,
    response: std::result::Result<ClientRequestResult, oneshot::error::RecvError>,
) -> Option<CoreRequestPermissionsResponse> {
    let value = match response {
        Ok(Ok(value)) => value,
        Ok(Err(err)) if is_turn_transition_server_request_error(&err) => return None,
        Ok(Err(err)) => {
            error!("request failed with client error: {err:?}");
            return Some(CoreRequestPermissionsResponse {
                permissions: Default::default(),
                scope: CorePermissionGrantScope::Turn,
            });
        }
        Err(err) => {
            error!("request failed: {err:?}");
            return Some(CoreRequestPermissionsResponse {
                permissions: Default::default(),
                scope: CorePermissionGrantScope::Turn,
            });
        }
    };

    let response = serde_json::from_value::<PermissionsRequestApprovalResponse>(value)
        .unwrap_or_else(|err| {
            error!("failed to deserialize PermissionsRequestApprovalResponse: {err}");
            PermissionsRequestApprovalResponse {
                permissions: ApiGrantedPermissionProfile::default(),
                scope: praxis_app_gateway_protocol::PermissionGrantScope::Turn,
            }
        });
    Some(CoreRequestPermissionsResponse {
        permissions: intersect_permission_profiles(
            requested_permissions.into(),
            response.permissions.into(),
        )
        .into(),
        scope: response.scope.to_core(),
    })
}

fn map_file_change_approval_decision(
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

#[allow(clippy::too_many_arguments)]
async fn on_file_change_request_approval_response(
    event_turn_id: String,
    conversation_id: ThreadId,
    item_id: String,
    changes: Vec<FileUpdateChange>,
    pending_request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
    codex: Arc<PraxisThread>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state: Arc<Mutex<ThreadState>>,
    permission_guard: ThreadWatchActiveGuard,
) {
    let response = receiver.await;
    resolve_server_request_on_thread_listener(&thread_state, pending_request_id).await;
    drop(permission_guard);
    let (decision, completion_status) = match response {
        Ok(Ok(value)) => {
            let response = serde_json::from_value::<FileChangeRequestApprovalResponse>(value)
                .unwrap_or_else(|err| {
                    error!("failed to deserialize FileChangeRequestApprovalResponse: {err}");
                    FileChangeRequestApprovalResponse {
                        decision: FileChangeApprovalDecision::Decline,
                    }
                });

            let (decision, completion_status) =
                map_file_change_approval_decision(response.decision);
            // Allow EventMsg::PatchApplyEnd to emit ItemCompleted for accepted patches.
            // Only short-circuit on declines/cancels/failures.
            (decision, completion_status)
        }
        Ok(Err(err)) if is_turn_transition_server_request_error(&err) => return,
        Ok(Err(err)) => {
            error!("request failed with client error: {err:?}");
            (ReviewDecision::Denied, Some(PatchApplyStatus::Failed))
        }
        Err(err) => {
            error!("request failed: {err:?}");
            (ReviewDecision::Denied, Some(PatchApplyStatus::Failed))
        }
    };

    if let Some(status) = completion_status {
        complete_file_change_item(
            conversation_id,
            item_id.clone(),
            changes,
            status,
            event_turn_id.clone(),
            &outgoing,
            &thread_state,
        )
        .await;
    }

    if let Err(err) = codex
        .submit(Op::PatchApproval {
            id: item_id,
            decision,
        })
        .await
    {
        error!("failed to submit PatchApproval: {err}");
    }
}

#[allow(clippy::too_many_arguments)]
async fn on_command_execution_request_approval_response(
    event_turn_id: String,
    conversation_id: ThreadId,
    approval_id: Option<String>,
    item_id: String,
    completion_item: Option<CommandExecutionCompletionItem>,
    pending_request_id: RequestId,
    receiver: oneshot::Receiver<ClientRequestResult>,
    conversation: Arc<PraxisThread>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state: Arc<Mutex<ThreadState>>,
    permission_guard: ThreadWatchActiveGuard,
) {
    let response = receiver.await;
    resolve_server_request_on_thread_listener(&thread_state, pending_request_id).await;
    drop(permission_guard);
    let (decision, completion_status) = match response {
        Ok(Ok(value)) => {
            let response = serde_json::from_value::<CommandExecutionRequestApprovalResponse>(value)
                .unwrap_or_else(|err| {
                    error!("failed to deserialize CommandExecutionRequestApprovalResponse: {err}");
                    CommandExecutionRequestApprovalResponse {
                        decision: CommandExecutionApprovalDecision::Decline,
                    }
                });

            let decision = response.decision;

            let (decision, completion_status) = match decision {
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
                        ApiNetworkPolicyRuleAction::Allow => None,
                        ApiNetworkPolicyRuleAction::Deny => Some(CommandExecutionStatus::Declined),
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
            };
            (decision, completion_status)
        }
        Ok(Err(err)) if is_turn_transition_server_request_error(&err) => return,
        Ok(Err(err)) => {
            error!("request failed with client error: {err:?}");
            (ReviewDecision::Denied, Some(CommandExecutionStatus::Failed))
        }
        Err(err) => {
            error!("request failed: {err:?}");
            (ReviewDecision::Denied, Some(CommandExecutionStatus::Failed))
        }
    };

    let suppress_subcommand_completion_item = {
        // For regular shell/unified_exec approvals, approval_id is null.
        // For zsh-fork subcommand approvals, approval_id is present and
        // item_id points to the parent command item.
        if approval_id.is_some() {
            let state = thread_state.lock().await;
            state
                .turn_summary
                .command_execution_started
                .contains(&item_id)
        } else {
            false
        }
    };

    if let Some(status) = completion_status
        && !suppress_subcommand_completion_item
        && let Some(completion_item) = completion_item
    {
        complete_command_execution_item(
            conversation_id,
            event_turn_id.clone(),
            item_id.clone(),
            completion_item.command,
            completion_item.cwd,
            /*process_id*/ None,
            CommandExecutionSource::Agent,
            completion_item.command_actions,
            status,
            &outgoing,
        )
        .await;
    }

    if let Err(err) = conversation
        .submit(Op::ExecApproval {
            id: approval_id.unwrap_or_else(|| item_id.clone()),
            turn_id: Some(event_turn_id),
            decision,
        })
        .await
    {
        error!("failed to submit ExecApproval: {err}");
    }
}

fn collab_resume_begin_item(
    begin_event: praxis_protocol::protocol::CollabResumeBeginEvent,
) -> ThreadItem {
    ThreadItem::CollabAgentToolCall {
        id: begin_event.call_id,
        tool: CollabAgentTool::ResumeThread,
        status: ApiCollabToolCallStatus::InProgress,
        sender_thread_id: begin_event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![begin_event.receiver_thread_id.to_string()],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states: HashMap::new(),
    }
}

fn collab_resume_end_item(
    end_event: praxis_protocol::protocol::CollabResumeEndEvent,
) -> ThreadItem {
    let status = match &end_event.status {
        praxis_protocol::protocol::AgentStatus::Errored(_)
        | praxis_protocol::protocol::AgentStatus::NotFound => ApiCollabToolCallStatus::Failed,
        _ => ApiCollabToolCallStatus::Completed,
    };
    let receiver_id = end_event.receiver_thread_id.to_string();
    let agents_states = [(
        receiver_id.clone(),
        ApiCollabAgentStatus::from(end_event.status),
    )]
    .into_iter()
    .collect();
    ThreadItem::CollabAgentToolCall {
        id: end_event.call_id,
        tool: CollabAgentTool::ResumeThread,
        status,
        sender_thread_id: end_event.sender_thread_id.to_string(),
        receiver_thread_ids: vec![receiver_id],
        prompt: None,
        model: None,
        reasoning_effort: None,
        agents_states,
    }
}

/// similar to handle_mcp_tool_call_begin in exec
async fn construct_mcp_tool_call_notification(
    begin_event: McpToolCallBeginEvent,
    thread_id: String,
    turn_id: String,
) -> ItemStartedNotification {
    let item = ThreadItem::McpToolCall {
        id: begin_event.call_id,
        server: begin_event.invocation.server,
        tool: begin_event.invocation.tool,
        status: McpToolCallStatus::InProgress,
        arguments: begin_event.invocation.arguments.unwrap_or(JsonValue::Null),
        result: None,
        error: None,
        duration_ms: None,
    };
    ItemStartedNotification {
        thread_id,
        turn_id,
        item,
    }
}

/// similar to handle_mcp_tool_call_end in exec
async fn construct_mcp_tool_call_end_notification(
    end_event: McpToolCallEndEvent,
    thread_id: String,
    turn_id: String,
) -> ItemCompletedNotification {
    let status = if end_event.is_success() {
        McpToolCallStatus::Completed
    } else {
        McpToolCallStatus::Failed
    };
    let duration_ms = i64::try_from(end_event.duration.as_millis()).ok();

    let (result, error) = match &end_event.result {
        Ok(value) => (
            Some(McpToolCallResult {
                content: value.content.clone(),
                structured_content: value.structured_content.clone(),
            }),
            None,
        ),
        Err(message) => (
            None,
            Some(McpToolCallError {
                message: message.clone(),
            }),
        ),
    };

    let item = ThreadItem::McpToolCall {
        id: end_event.call_id,
        server: end_event.invocation.server,
        tool: end_event.invocation.tool,
        status,
        arguments: end_event.invocation.arguments.unwrap_or(JsonValue::Null),
        result,
        error,
        duration_ms,
    };
    ItemCompletedNotification {
        thread_id,
        turn_id,
        item,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHANNEL_CAPACITY;
    use crate::outgoing_message::ConnectionId;
    use crate::outgoing_message::OutgoingEnvelope;
    use crate::outgoing_message::OutgoingMessage;
    use crate::outgoing_message::OutgoingMessageSender;
    use anyhow::Result;
    use anyhow::anyhow;
    use anyhow::bail;
    use praxis_app_gateway_protocol::GuardianApprovalReviewStatus;
    use praxis_app_gateway_protocol::JSONRPCErrorError;
    use praxis_app_gateway_protocol::TurnPlanStepStatus;
    use praxis_protocol::items::HookPromptFragment;
    use praxis_protocol::items::build_hook_prompt_message;
    use praxis_protocol::mcp::CallToolResult;
    use praxis_protocol::models::FileSystemPermissions as CoreFileSystemPermissions;
    use praxis_protocol::models::NetworkPermissions as CoreNetworkPermissions;
    use praxis_protocol::plan_tool::PlanItemArg;
    use praxis_protocol::plan_tool::StepStatus;
    use praxis_protocol::protocol::CollabResumeBeginEvent;
    use praxis_protocol::protocol::CollabResumeEndEvent;
    use praxis_protocol::protocol::CreditsSnapshot;
    use praxis_protocol::protocol::McpInvocation;
    use praxis_protocol::protocol::RateLimitSnapshot;
    use praxis_protocol::protocol::RateLimitWindow;
    use praxis_protocol::protocol::TokenUsage;
    use praxis_protocol::protocol::TokenUsageInfo;
    use praxis_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use rmcp::model::Content;
    use serde_json::Value as JsonValue;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio::sync::mpsc;

    fn new_thread_state() -> Arc<Mutex<ThreadState>> {
        Arc::new(Mutex::new(ThreadState::default()))
    }

    async fn recv_broadcast_message(
        rx: &mut mpsc::Receiver<OutgoingEnvelope>,
    ) -> Result<OutgoingMessage> {
        let envelope = rx
            .recv()
            .await
            .ok_or_else(|| anyhow!("should send one message"))?;
        match envelope {
            OutgoingEnvelope::Broadcast { message } => Ok(message),
            OutgoingEnvelope::ToConnection { message, .. } => Ok(message),
        }
    }

    #[test]
    fn guardian_assessment_started_uses_event_turn_id_fallback() {
        let conversation_id = ThreadId::new();
        let action = praxis_protocol::protocol::GuardianAssessmentAction::Command {
            source: praxis_protocol::protocol::GuardianCommandSource::Shell,
            command: "rm -rf /tmp/example.sqlite".to_string(),
            cwd: "/tmp".into(),
        };
        let notification = guardian_auto_approval_review_notification(
            &conversation_id,
            "turn-from-event",
            &GuardianAssessmentEvent {
                id: "item-1".to_string(),
                turn_id: String::new(),
                status: praxis_protocol::protocol::GuardianAssessmentStatus::InProgress,
                risk_score: None,
                risk_level: None,
                rationale: None,
                action: action.clone(),
            },
        );

        match notification {
            ServerNotification::ItemGuardianApprovalReviewStarted(payload) => {
                assert_eq!(payload.thread_id, conversation_id.to_string());
                assert_eq!(payload.turn_id, "turn-from-event");
                assert_eq!(payload.target_item_id, "item-1");
                assert_eq!(
                    payload.review.status,
                    GuardianApprovalReviewStatus::InProgress
                );
                assert_eq!(payload.review.risk_score, None);
                assert_eq!(payload.review.risk_level, None);
                assert_eq!(payload.review.rationale, None);
                assert_eq!(payload.action, action.into());
            }
            other => panic!("unexpected notification: {other:?}"),
        }
    }

    #[test]
    fn guardian_assessment_completed_emits_review_payload() {
        let conversation_id = ThreadId::new();
        let action = praxis_protocol::protocol::GuardianAssessmentAction::Command {
            source: praxis_protocol::protocol::GuardianCommandSource::Shell,
            command: "rm -rf /tmp/example.sqlite".to_string(),
            cwd: "/tmp".into(),
        };
        let notification = guardian_auto_approval_review_notification(
            &conversation_id,
            "turn-from-event",
            &GuardianAssessmentEvent {
                id: "item-2".to_string(),
                turn_id: "turn-from-assessment".to_string(),
                status: praxis_protocol::protocol::GuardianAssessmentStatus::Denied,
                risk_score: Some(91),
                risk_level: Some(praxis_protocol::protocol::GuardianRiskLevel::High),
                rationale: Some("too risky".to_string()),
                action: action.clone(),
            },
        );

        match notification {
            ServerNotification::ItemGuardianApprovalReviewCompleted(payload) => {
                assert_eq!(payload.thread_id, conversation_id.to_string());
                assert_eq!(payload.turn_id, "turn-from-assessment");
                assert_eq!(payload.target_item_id, "item-2");
                assert_eq!(payload.review.status, GuardianApprovalReviewStatus::Denied);
                assert_eq!(payload.review.risk_score, Some(91));
                assert_eq!(
                    payload.review.risk_level,
                    Some(praxis_app_gateway_protocol::GuardianRiskLevel::High)
                );
                assert_eq!(payload.review.rationale.as_deref(), Some("too risky"));
                assert_eq!(payload.action, action.into());
            }
            other => panic!("unexpected notification: {other:?}"),
        }
    }

    #[test]
    fn guardian_assessment_aborted_emits_completed_review_payload() {
        let conversation_id = ThreadId::new();
        let action = praxis_protocol::protocol::GuardianAssessmentAction::NetworkAccess {
            target: "api.openai.com:443".to_string(),
            host: "api.openai.com".to_string(),
            protocol: praxis_protocol::protocol::NetworkApprovalProtocol::Https,
            port: 443,
        };
        let notification = guardian_auto_approval_review_notification(
            &conversation_id,
            "turn-from-event",
            &GuardianAssessmentEvent {
                id: "item-3".to_string(),
                turn_id: "turn-from-assessment".to_string(),
                status: praxis_protocol::protocol::GuardianAssessmentStatus::Aborted,
                risk_score: None,
                risk_level: None,
                rationale: None,
                action: action.clone(),
            },
        );

        match notification {
            ServerNotification::ItemGuardianApprovalReviewCompleted(payload) => {
                assert_eq!(payload.thread_id, conversation_id.to_string());
                assert_eq!(payload.turn_id, "turn-from-assessment");
                assert_eq!(payload.target_item_id, "item-3");
                assert_eq!(payload.review.status, GuardianApprovalReviewStatus::Aborted);
                assert_eq!(payload.review.risk_score, None);
                assert_eq!(payload.review.risk_level, None);
                assert_eq!(payload.review.rationale, None);
                assert_eq!(payload.action, action.into());
            }
            other => panic!("unexpected notification: {other:?}"),
        }
    }

    #[test]
    fn file_change_accept_for_session_maps_to_approved_for_session() {
        let (decision, completion_status) =
            map_file_change_approval_decision(FileChangeApprovalDecision::AcceptForSession);
        assert_eq!(decision, ReviewDecision::ApprovedForSession);
        assert_eq!(completion_status, None);
    }

    #[test]
    fn mcp_server_elicitation_turn_transition_error_maps_to_cancel() {
        let error = JSONRPCErrorError {
            code: -1,
            message: "client request resolved because the turn state was changed".to_string(),
            data: Some(serde_json::json!({ "reason": "turnTransition" })),
        };

        let response = mcp_server_elicitation_response_from_client_result(Ok(Err(error)));

        assert_eq!(
            response,
            McpServerElicitationRequestResponse {
                action: McpServerElicitationAction::Cancel,
                content: None,
                meta: None,
            }
        );
    }

    #[test]
    fn request_permissions_turn_transition_error_is_ignored() {
        let error = JSONRPCErrorError {
            code: -1,
            message: "client request resolved because the turn state was changed".to_string(),
            data: Some(serde_json::json!({ "reason": "turnTransition" })),
        };

        let response = request_permissions_response_from_client_result(
            CoreRequestPermissionProfile::default(),
            Ok(Err(error)),
        );

        assert_eq!(response, None);
    }

    #[test]
    fn request_permissions_response_accepts_partial_network_and_file_system_grants() {
        let input_path = if cfg!(target_os = "windows") {
            r"C:\tmp\input"
        } else {
            "/tmp/input"
        };
        let output_path = if cfg!(target_os = "windows") {
            r"C:\tmp\output"
        } else {
            "/tmp/output"
        };
        let ignored_path = if cfg!(target_os = "windows") {
            r"C:\tmp\ignored"
        } else {
            "/tmp/ignored"
        };
        let absolute_path = |path: &str| {
            AbsolutePathBuf::try_from(std::path::PathBuf::from(path)).expect("absolute path")
        };
        let requested_permissions = CoreRequestPermissionProfile {
            network: Some(CoreNetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(CoreFileSystemPermissions {
                read: Some(vec![absolute_path(input_path)]),
                write: Some(vec![absolute_path(output_path)]),
            }),
        };
        let cases = vec![
            (
                serde_json::json!({}),
                CoreRequestPermissionProfile::default(),
            ),
            (
                serde_json::json!({
                    "network": {
                        "enabled": true,
                    },
                }),
                CoreRequestPermissionProfile {
                    network: Some(CoreNetworkPermissions {
                        enabled: Some(true),
                    }),
                    ..CoreRequestPermissionProfile::default()
                },
            ),
            (
                serde_json::json!({
                    "fileSystem": {
                        "write": [output_path],
                    },
                }),
                CoreRequestPermissionProfile {
                    file_system: Some(CoreFileSystemPermissions {
                        read: None,
                        write: Some(vec![absolute_path(output_path)]),
                    }),
                    ..CoreRequestPermissionProfile::default()
                },
            ),
            (
                serde_json::json!({
                    "fileSystem": {
                        "read": [input_path],
                        "write": [output_path, ignored_path],
                    },
                    "macos": {
                        "calendar": true,
                    },
                }),
                CoreRequestPermissionProfile {
                    file_system: Some(CoreFileSystemPermissions {
                        read: Some(vec![absolute_path(input_path)]),
                        write: Some(vec![absolute_path(output_path)]),
                    }),
                    ..CoreRequestPermissionProfile::default()
                },
            ),
        ];

        for (granted_permissions, expected_permissions) in cases {
            let response = request_permissions_response_from_client_result(
                requested_permissions.clone(),
                Ok(Ok(serde_json::json!({
                    "permissions": granted_permissions,
                }))),
            )
            .expect("response should be accepted");

            assert_eq!(
                response,
                CoreRequestPermissionsResponse {
                    permissions: expected_permissions,
                    scope: CorePermissionGrantScope::Turn,
                }
            );
        }
    }

    #[test]
    fn request_permissions_response_preserves_session_scope() {
        let response = request_permissions_response_from_client_result(
            CoreRequestPermissionProfile::default(),
            Ok(Ok(serde_json::json!({
                "scope": "session",
                "permissions": {},
            }))),
        )
        .expect("response should be accepted");

        assert_eq!(
            response,
            CoreRequestPermissionsResponse {
                permissions: CoreRequestPermissionProfile::default(),
                scope: CorePermissionGrantScope::Session,
            }
        );
    }

    #[test]
    fn collab_resume_begin_maps_to_item_started_resume_thread() {
        let event = CollabResumeBeginEvent {
            call_id: "call-1".to_string(),
            sender_thread_id: ThreadId::new(),
            receiver_thread_id: ThreadId::new(),
            receiver_agent_base_name: None,
            receiver_agent_title: None,
            receiver_agent_display_name: None,
            receiver_agent_role: None,
        };

        let item = collab_resume_begin_item(event.clone());
        let expected = ThreadItem::CollabAgentToolCall {
            id: event.call_id,
            tool: CollabAgentTool::ResumeThread,
            status: ApiCollabToolCallStatus::InProgress,
            sender_thread_id: event.sender_thread_id.to_string(),
            receiver_thread_ids: vec![event.receiver_thread_id.to_string()],
            prompt: None,
            model: None,
            reasoning_effort: None,
            agents_states: HashMap::new(),
        };
        assert_eq!(item, expected);
    }

    #[test]
    fn collab_resume_end_maps_to_item_completed_resume_thread() {
        let event = CollabResumeEndEvent {
            call_id: "call-2".to_string(),
            sender_thread_id: ThreadId::new(),
            receiver_thread_id: ThreadId::new(),
            receiver_agent_base_name: None,
            receiver_agent_title: None,
            receiver_agent_display_name: None,
            receiver_agent_role: None,
            status: praxis_protocol::protocol::AgentStatus::NotFound,
        };

        let item = collab_resume_end_item(event.clone());
        let receiver_id = event.receiver_thread_id.to_string();
        let expected = ThreadItem::CollabAgentToolCall {
            id: event.call_id,
            tool: CollabAgentTool::ResumeThread,
            status: ApiCollabToolCallStatus::Failed,
            sender_thread_id: event.sender_thread_id.to_string(),
            receiver_thread_ids: vec![receiver_id.clone()],
            prompt: None,
            model: None,
            reasoning_effort: None,
            agents_states: [(
                receiver_id,
                ApiCollabAgentStatus::from(praxis_protocol::protocol::AgentStatus::NotFound),
            )]
            .into_iter()
            .collect(),
        };
        assert_eq!(item, expected);
    }

    #[tokio::test]
    async fn test_handle_error_records_message() -> Result<()> {
        let conversation_id = ThreadId::new();
        let thread_state = new_thread_state();

        handle_error(
            conversation_id,
            TurnError {
                message: "boom".to_string(),
                praxis_error_info: Some(ApiCodexErrorInfo::InternalServerError),
                additional_details: None,
            },
            &thread_state,
        )
        .await;

        let turn_summary = find_and_remove_turn_summary(conversation_id, &thread_state).await;
        assert_eq!(
            turn_summary.last_error,
            Some(TurnError {
                message: "boom".to_string(),
                praxis_error_info: Some(ApiCodexErrorInfo::InternalServerError),
                additional_details: None,
            })
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_turn_complete_emits_completed_without_error() -> Result<()> {
        let conversation_id = ThreadId::new();
        let event_turn_id = "complete1".to_string();
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );
        let thread_state = new_thread_state();

        handle_turn_complete(
            conversation_id,
            event_turn_id.clone(),
            &outgoing,
            &thread_state,
        )
        .await;

        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
                assert_eq!(n.turn.id, event_turn_id);
                assert_eq!(n.turn.status, TurnStatus::Completed);
                assert_eq!(n.turn.error, None);
            }
            other => bail!("unexpected message: {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_turn_interrupted_emits_interrupted_with_error() -> Result<()> {
        let conversation_id = ThreadId::new();
        let event_turn_id = "interrupt1".to_string();
        let thread_state = new_thread_state();
        handle_error(
            conversation_id,
            TurnError {
                message: "oops".to_string(),
                praxis_error_info: None,
                additional_details: None,
            },
            &thread_state,
        )
        .await;
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );

        handle_turn_interrupted(
            conversation_id,
            event_turn_id.clone(),
            &outgoing,
            &thread_state,
        )
        .await;

        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
                assert_eq!(n.turn.id, event_turn_id);
                assert_eq!(n.turn.status, TurnStatus::Interrupted);
                assert_eq!(n.turn.error, None);
            }
            other => bail!("unexpected message: {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_turn_complete_emits_failed_with_error() -> Result<()> {
        let conversation_id = ThreadId::new();
        let event_turn_id = "complete_err1".to_string();
        let thread_state = new_thread_state();
        handle_error(
            conversation_id,
            TurnError {
                message: "bad".to_string(),
                praxis_error_info: Some(ApiCodexErrorInfo::Other),
                additional_details: None,
            },
            &thread_state,
        )
        .await;
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );

        handle_turn_complete(
            conversation_id,
            event_turn_id.clone(),
            &outgoing,
            &thread_state,
        )
        .await;

        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
                assert_eq!(n.turn.id, event_turn_id);
                assert_eq!(n.turn.status, TurnStatus::Failed);
                assert_eq!(
                    n.turn.error,
                    Some(TurnError {
                        message: "bad".to_string(),
                        praxis_error_info: Some(ApiCodexErrorInfo::Other),
                        additional_details: None,
                    })
                );
            }
            other => bail!("unexpected message: {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_turn_plan_update_emits_notification() -> Result<()> {
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );
        let update = UpdatePlanArgs {
            explanation: Some("need plan".to_string()),
            plan: vec![
                PlanItemArg {
                    step: "first".to_string(),
                    status: StepStatus::Pending,
                },
                PlanItemArg {
                    step: "second".to_string(),
                    status: StepStatus::Completed,
                },
            ],
        };

        let conversation_id = ThreadId::new();

        handle_turn_plan_update(conversation_id, "turn-123", update, &outgoing).await;

        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnPlanUpdated(n)) => {
                assert_eq!(n.thread_id, conversation_id.to_string());
                assert_eq!(n.turn_id, "turn-123");
                assert_eq!(n.explanation.as_deref(), Some("need plan"));
                assert_eq!(n.plan.len(), 2);
                assert_eq!(n.plan[0].step, "first");
                assert_eq!(n.plan[0].status, TurnPlanStepStatus::Pending);
                assert_eq!(n.plan[1].step, "second");
                assert_eq!(n.plan[1].status, TurnPlanStepStatus::Completed);
            }
            other => bail!("unexpected message: {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_token_count_event_emits_usage_and_rate_limits() -> Result<()> {
        let conversation_id = ThreadId::new();
        let turn_id = "turn-123".to_string();
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );

        let info = TokenUsageInfo {
            total_token_usage: TokenUsage {
                input_tokens: 100,
                cached_input_tokens: 25,
                cache_reported_input_tokens: 100,
                output_tokens: 50,
                reasoning_output_tokens: 9,
                total_tokens: 200,
            },
            last_token_usage: TokenUsage {
                input_tokens: 10,
                cached_input_tokens: 5,
                cache_reported_input_tokens: 10,
                output_tokens: 7,
                reasoning_output_tokens: 1,
                total_tokens: 23,
            },
            model_context_window: Some(4096),
            model_auto_compact_token_limit: Some(3600),
        };
        let rate_limits = RateLimitSnapshot {
            limit_id: Some("codex".to_string()),
            limit_name: None,
            primary: Some(RateLimitWindow {
                used_percent: 42.5,
                window_minutes: Some(15),
                resets_at: Some(1700000000),
            }),
            secondary: None,
            credits: Some(CreditsSnapshot {
                has_credits: true,
                unlimited: false,
                balance: Some("5".to_string()),
            }),
            plan_type: None,
        };

        handle_token_count_event(
            conversation_id,
            turn_id.clone(),
            TokenCountEvent {
                info: Some(info),
                rate_limits: Some(rate_limits),
            },
            &outgoing,
        )
        .await;

        let first = recv_broadcast_message(&mut rx).await?;
        match first {
            OutgoingMessage::AppGatewayNotification(
                ServerNotification::ThreadTokenUsageUpdated(payload),
            ) => {
                assert_eq!(payload.thread_id, conversation_id.to_string());
                assert_eq!(payload.turn_id, turn_id);
                let usage = payload.token_usage;
                assert_eq!(usage.total.total_tokens, 200);
                assert_eq!(usage.total.cached_input_tokens, 25);
                assert_eq!(usage.total.cache_reported_input_tokens, 100);
                assert_eq!(usage.last.output_tokens, 7);
                assert_eq!(usage.model_context_window, Some(4096));
            }
            other => bail!("unexpected notification: {other:?}"),
        }

        let second = recv_broadcast_message(&mut rx).await?;
        match second {
            OutgoingMessage::AppGatewayNotification(
                ServerNotification::AccountRateLimitsUpdated(payload),
            ) => {
                assert_eq!(payload.rate_limits.limit_id.as_deref(), Some("codex"));
                assert_eq!(payload.rate_limits.limit_name, None);
                assert!(payload.rate_limits.primary.is_some());
                assert!(payload.rate_limits.credits.is_some());
            }
            other => bail!("unexpected notification: {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_handle_token_count_event_without_usage_info() -> Result<()> {
        let conversation_id = ThreadId::new();
        let turn_id = "turn-456".to_string();
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );

        handle_token_count_event(
            conversation_id,
            turn_id.clone(),
            TokenCountEvent {
                info: None,
                rate_limits: None,
            },
            &outgoing,
        )
        .await;

        assert!(
            rx.try_recv().is_err(),
            "no notifications should be emitted when token usage info is absent"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_construct_mcp_tool_call_begin_notification_with_args() {
        let begin_event = McpToolCallBeginEvent {
            call_id: "call_123".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: Some(serde_json::json!({"server": ""})),
            },
        };

        let thread_id = ThreadId::new().to_string();
        let turn_id = "turn_1".to_string();
        let notification = construct_mcp_tool_call_notification(
            begin_event.clone(),
            thread_id.clone(),
            turn_id.clone(),
        )
        .await;

        let expected = ItemStartedNotification {
            thread_id,
            turn_id,
            item: ThreadItem::McpToolCall {
                id: begin_event.call_id,
                server: begin_event.invocation.server,
                tool: begin_event.invocation.tool,
                status: McpToolCallStatus::InProgress,
                arguments: serde_json::json!({"server": ""}),
                result: None,
                error: None,
                duration_ms: None,
            },
        };

        assert_eq!(notification, expected);
    }

    #[tokio::test]
    async fn test_handle_turn_complete_emits_error_multiple_turns() -> Result<()> {
        // Conversation A will have two turns; Conversation B will have one turn.
        let conversation_a = ThreadId::new();
        let conversation_b = ThreadId::new();
        let thread_state = new_thread_state();

        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );

        // Turn 1 on conversation A
        let a_turn1 = "a_turn1".to_string();
        handle_error(
            conversation_a,
            TurnError {
                message: "a1".to_string(),
                praxis_error_info: Some(ApiCodexErrorInfo::BadRequest),
                additional_details: None,
            },
            &thread_state,
        )
        .await;
        handle_turn_complete(conversation_a, a_turn1.clone(), &outgoing, &thread_state).await;

        // Turn 1 on conversation B
        let b_turn1 = "b_turn1".to_string();
        handle_error(
            conversation_b,
            TurnError {
                message: "b1".to_string(),
                praxis_error_info: None,
                additional_details: None,
            },
            &thread_state,
        )
        .await;
        handle_turn_complete(conversation_b, b_turn1.clone(), &outgoing, &thread_state).await;

        // Turn 2 on conversation A
        let a_turn2 = "a_turn2".to_string();
        handle_turn_complete(conversation_a, a_turn2.clone(), &outgoing, &thread_state).await;

        // Verify: A turn 1
        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
                assert_eq!(n.turn.id, a_turn1);
                assert_eq!(n.turn.status, TurnStatus::Failed);
                assert_eq!(
                    n.turn.error,
                    Some(TurnError {
                        message: "a1".to_string(),
                        praxis_error_info: Some(ApiCodexErrorInfo::BadRequest),
                        additional_details: None,
                    })
                );
            }
            other => bail!("unexpected message: {other:?}"),
        }

        // Verify: B turn 1
        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
                assert_eq!(n.turn.id, b_turn1);
                assert_eq!(n.turn.status, TurnStatus::Failed);
                assert_eq!(
                    n.turn.error,
                    Some(TurnError {
                        message: "b1".to_string(),
                        praxis_error_info: None,
                        additional_details: None,
                    })
                );
            }
            other => bail!("unexpected message: {other:?}"),
        }

        // Verify: A turn 2
        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnCompleted(n)) => {
                assert_eq!(n.turn.id, a_turn2);
                assert_eq!(n.turn.status, TurnStatus::Completed);
                assert_eq!(n.turn.error, None);
            }
            other => bail!("unexpected message: {other:?}"),
        }

        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_construct_mcp_tool_call_begin_notification_without_args() {
        let begin_event = McpToolCallBeginEvent {
            call_id: "call_456".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: None,
            },
        };

        let thread_id = ThreadId::new().to_string();
        let turn_id = "turn_2".to_string();
        let notification = construct_mcp_tool_call_notification(
            begin_event.clone(),
            thread_id.clone(),
            turn_id.clone(),
        )
        .await;

        let expected = ItemStartedNotification {
            thread_id,
            turn_id,
            item: ThreadItem::McpToolCall {
                id: begin_event.call_id,
                server: begin_event.invocation.server,
                tool: begin_event.invocation.tool,
                status: McpToolCallStatus::InProgress,
                arguments: JsonValue::Null,
                result: None,
                error: None,
                duration_ms: None,
            },
        };

        assert_eq!(notification, expected);
    }

    #[tokio::test]
    async fn test_construct_mcp_tool_call_end_notification_success() {
        let content = vec![
            serde_json::to_value(Content::text("{\"resources\":[]}"))
                .expect("content should serialize"),
        ];
        let result = CallToolResult {
            content: content.clone(),
            is_error: Some(false),
            structured_content: None,
            meta: None,
        };

        let end_event = McpToolCallEndEvent {
            call_id: "call_789".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: Some(serde_json::json!({"server": ""})),
            },
            duration: Duration::from_nanos(92708),
            result: Ok(result),
        };

        let thread_id = ThreadId::new().to_string();
        let turn_id = "turn_3".to_string();
        let notification = construct_mcp_tool_call_end_notification(
            end_event.clone(),
            thread_id.clone(),
            turn_id.clone(),
        )
        .await;

        let expected = ItemCompletedNotification {
            thread_id,
            turn_id,
            item: ThreadItem::McpToolCall {
                id: end_event.call_id,
                server: end_event.invocation.server,
                tool: end_event.invocation.tool,
                status: McpToolCallStatus::Completed,
                arguments: serde_json::json!({"server": ""}),
                result: Some(McpToolCallResult {
                    content,
                    structured_content: None,
                }),
                error: None,
                duration_ms: Some(0),
            },
        };

        assert_eq!(notification, expected);
    }

    #[tokio::test]
    async fn test_construct_mcp_tool_call_end_notification_error() {
        let end_event = McpToolCallEndEvent {
            call_id: "call_err".to_string(),
            invocation: McpInvocation {
                server: "codex".to_string(),
                tool: "list_mcp_resources".to_string(),
                arguments: None,
            },
            duration: Duration::from_millis(1),
            result: Err("boom".to_string()),
        };

        let thread_id = ThreadId::new().to_string();
        let turn_id = "turn_4".to_string();
        let notification = construct_mcp_tool_call_end_notification(
            end_event.clone(),
            thread_id.clone(),
            turn_id.clone(),
        )
        .await;

        let expected = ItemCompletedNotification {
            thread_id,
            turn_id,
            item: ThreadItem::McpToolCall {
                id: end_event.call_id,
                server: end_event.invocation.server,
                tool: end_event.invocation.tool,
                status: McpToolCallStatus::Failed,
                arguments: JsonValue::Null,
                result: None,
                error: Some(McpToolCallError {
                    message: "boom".to_string(),
                }),
                duration_ms: Some(1),
            },
        };

        assert_eq!(notification, expected);
    }

    #[tokio::test]
    async fn test_handle_turn_diff_emits_API_notification() -> Result<()> {
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            ThreadId::new(),
        );
        let unified_diff = "--- a\n+++ b\n".to_string();
        let conversation_id = ThreadId::new();

        handle_turn_diff(
            conversation_id,
            "turn-1",
            TurnDiffEvent {
                unified_diff: unified_diff.clone(),
            },
            &outgoing,
        )
        .await;

        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::TurnDiffUpdated(
                notification,
            )) => {
                assert_eq!(notification.thread_id, conversation_id.to_string());
                assert_eq!(notification.turn_id, "turn-1");
                assert_eq!(notification.diff, unified_diff);
            }
            other => bail!("unexpected message: {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }

    #[tokio::test]
    async fn test_hook_prompt_raw_response_emits_item_completed() -> Result<()> {
        let (tx, mut rx) = mpsc::channel(CHANNEL_CAPACITY);
        let outgoing = Arc::new(OutgoingMessageSender::new(tx));
        let conversation_id = ThreadId::new();
        let outgoing = ThreadScopedOutgoingMessageSender::new(
            outgoing,
            vec![ConnectionId(1)],
            conversation_id,
        );
        let item = build_hook_prompt_message(&[
            HookPromptFragment::from_single_hook("Retry with tests.", "hook-run-1"),
            HookPromptFragment::from_single_hook("Then summarize cleanly.", "hook-run-2"),
        ])
        .expect("hook prompt message");

        maybe_emit_hook_prompt_item_completed(conversation_id, "turn-1", &item, &outgoing).await;

        let msg = recv_broadcast_message(&mut rx).await?;
        match msg {
            OutgoingMessage::AppGatewayNotification(ServerNotification::ItemCompleted(
                notification,
            )) => {
                assert_eq!(notification.thread_id, conversation_id.to_string());
                assert_eq!(notification.turn_id, "turn-1");
                assert_eq!(
                    notification.item,
                    ThreadItem::HookPrompt {
                        id: notification.item.id().to_string(),
                        fragments: vec![
                            praxis_app_gateway_protocol::HookPromptFragment {
                                text: "Retry with tests.".into(),
                                hook_run_id: "hook-run-1".into(),
                            },
                            praxis_app_gateway_protocol::HookPromptFragment {
                                text: "Then summarize cleanly.".into(),
                                hook_run_id: "hook-run-2".into(),
                            },
                        ],
                    }
                );
            }
            other => bail!("unexpected message: {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "no extra messages expected");
        Ok(())
    }
}
