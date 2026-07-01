use super::*;

pub(super) async fn handle_turn_diff(
    conversation_id: ThreadId,
    event_turn_id: &str,
    turn_diff_event: TurnDiffEvent,
    outgoing: &ThreadScopedOutgoingMessageSender,
    root: PathBuf,
    workspace_change_store: &WorkspaceChangeStore,
) {
    let diff = turn_diff_event.unified_diff;
    {
        let notification = TurnDiffUpdatedNotification {
            thread_id: conversation_id.to_string(),
            turn_id: event_turn_id.to_string(),
            diff: diff.clone(),
        };
        outgoing
            .send_server_notification(ServerNotification::TurnDiffUpdated(notification))
            .await;
    }
    {
        let snapshot = workspace_change_store
            .update_from_diff(
                root,
                conversation_id.to_string(),
                Some(event_turn_id.to_string()),
                diff.as_str(),
            )
            .await;
        outgoing
            .send_server_notification(ServerNotification::WorkspaceChangeUpdated(
                WorkspaceChangeUpdatedNotification {
                    thread_id: conversation_id.to_string(),
                    snapshot,
                },
            ))
            .await;
    }
}

pub(super) async fn handle_turn_plan_update(
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

pub(super) async fn emit_turn_completed_with_status(
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

pub(super) async fn complete_file_change_item(
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
    ThreadItemNotificationSink::new(outgoing, &conversation_id, &turn_id)
        .item_completed(item)
        .await;
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn complete_command_execution_item(
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
    ThreadItemNotificationSink::new(outgoing, &conversation_id, &turn_id)
        .item_completed(item)
        .await;
}

pub(super) async fn maybe_emit_raw_response_item_completed(
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

pub(super) async fn maybe_emit_hook_prompt_item_completed(
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

    let item = ThreadItem::HookPrompt {
        id: hook_prompt.id,
        fragments: hook_prompt
            .fragments
            .into_iter()
            .map(praxis_app_gateway_protocol::HookPromptFragment::from)
            .collect(),
    };
    ThreadItemNotificationSink::new(outgoing, &conversation_id, turn_id)
        .item_completed(item)
        .await;
}

pub(super) async fn find_and_remove_turn_summary(
    _conversation_id: ThreadId,
    thread_state: &Arc<Mutex<ThreadState>>,
) -> TurnSummary {
    let mut state = thread_state.lock().await;
    std::mem::take(&mut state.turn_summary)
}

pub(super) async fn handle_turn_complete(
    conversation_id: ThreadId,
    event_turn_id: String,
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_state: &Arc<Mutex<ThreadState>>,
) -> (TurnStatus, Option<TurnError>) {
    let turn_summary = find_and_remove_turn_summary(conversation_id, thread_state).await;

    let (status, error) = match turn_summary.last_error {
        Some(error) => (TurnStatus::Failed, Some(error)),
        None => (TurnStatus::Completed, None),
    };

    emit_turn_completed_with_status(
        conversation_id,
        event_turn_id,
        status.clone(),
        error.clone(),
        outgoing,
    )
    .await;
    (status, error)
}

pub(super) async fn handle_turn_interrupted(
    conversation_id: ThreadId,
    event_turn_id: String,
    outgoing: &ThreadScopedOutgoingMessageSender,
    thread_state: &Arc<Mutex<ThreadState>>,
) -> (TurnStatus, Option<TurnError>) {
    find_and_remove_turn_summary(conversation_id, thread_state).await;

    emit_turn_completed_with_status(
        conversation_id,
        event_turn_id,
        TurnStatus::Interrupted,
        /*error*/ None,
        outgoing,
    )
    .await;
    (TurnStatus::Interrupted, None)
}

pub(super) async fn finish_automation_runs_for_turn(
    state_db: Option<&Arc<StateRuntime>>,
    conversation_id: &ThreadId,
    turn_id: &str,
    status: &TurnStatus,
    error: Option<&TurnError>,
    outgoing: &ThreadScopedOutgoingMessageSender,
) {
    let Some(state_db) = state_db else {
        return;
    };
    let run_status = match status {
        TurnStatus::Completed => AutomationRunStatus::Succeeded,
        TurnStatus::Failed => AutomationRunStatus::Failed,
        TurnStatus::Interrupted => AutomationRunStatus::Cancelled,
        TurnStatus::InProgress => return,
    };
    let error_message = error.map(|error| error.message.as_str());
    let thread_id = conversation_id.to_string();
    let runs = match state_db
        .finish_automation_runs_for_turn(thread_id.as_str(), turn_id, run_status, error_message)
        .await
    {
        Ok(runs) => runs,
        Err(err) => {
            tracing::warn!(
                thread_id = %conversation_id,
                turn_id,
                "failed to finish automation runs for turn: {err}"
            );
            return;
        }
    };
    for run in runs {
        outgoing
            .send_server_notification(ServerNotification::AutomationRunUpdated(
                AutomationRunUpdatedNotification {
                    run: api_automation_run_from_state(run),
                },
            ))
            .await;
    }
}

pub(super) async fn handle_thread_rollback_failed(
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

pub(super) async fn handle_token_count_event(
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

pub(super) async fn handle_error(
    _conversation_id: ThreadId,
    error: TurnError,
    thread_state: &Arc<Mutex<ThreadState>>,
) {
    let mut state = thread_state.lock().await;
    state.turn_summary.last_error = Some(error);
}

pub(super) async fn on_request_user_input_response(
    event_turn_id: String,
    pending_request: PendingServerRequest,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
    user_input_guard: ThreadWatchActiveGuard,
) {
    let response = pending_request
        .await_response_and_resolve(&thread_state, user_input_guard)
        .await;
    let Some(response) =
        try_decode_client_response_or_default::<ToolRequestUserInputResponse>(response, || {
            ToolRequestUserInputResponse {
                answers: HashMap::new(),
            }
        })
    else {
        return;
    };
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

pub(super) async fn on_mcp_server_elicitation_response(
    server_name: String,
    request_id: praxis_protocol::mcp::RequestId,
    pending_request: PendingServerRequest,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
    permission_guard: ThreadWatchActiveGuard,
) {
    let response = pending_request
        .await_response_and_resolve(&thread_state, permission_guard)
        .await;
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

pub(super) fn mcp_server_elicitation_response_from_client_result(
    response: PendingClientResponse,
) -> McpServerElicitationRequestResponse {
    match response_value_or_cancel(response) {
        ClientResponseValue::Value(value) => {
            decode_response_value_or_default::<McpServerElicitationRequestResponse>(value, || {
                McpServerElicitationRequestResponse {
                    action: McpServerElicitationAction::Decline,
                    content: None,
                    meta: None,
                }
            })
        }
        ClientResponseValue::TurnTransition => McpServerElicitationRequestResponse {
            action: McpServerElicitationAction::Cancel,
            content: None,
            meta: None,
        },
        ClientResponseValue::Fallback => McpServerElicitationRequestResponse {
            action: McpServerElicitationAction::Decline,
            content: None,
            meta: None,
        },
    }
}

pub(super) async fn on_request_permissions_response(
    call_id: String,
    requested_permissions: CoreRequestPermissionProfile,
    pending_request: PendingServerRequest,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
    request_permissions_guard: ThreadWatchActiveGuard,
) {
    let response = pending_request
        .await_response_and_resolve(&thread_state, request_permissions_guard)
        .await;
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

pub(super) fn request_permissions_response_from_client_result(
    requested_permissions: CoreRequestPermissionProfile,
    response: PendingClientResponse,
) -> Option<CoreRequestPermissionsResponse> {
    let response = try_decode_client_response_or_default::<PermissionsRequestApprovalResponse>(
        response,
        || PermissionsRequestApprovalResponse {
            permissions: ApiGrantedPermissionProfile::default(),
            scope: praxis_app_gateway_protocol::PermissionGrantScope::Turn,
        },
    )?;
    Some(CoreRequestPermissionsResponse {
        permissions: intersect_permission_profiles(
            requested_permissions.into(),
            response.permissions.into(),
        )
        .into(),
        scope: response.scope.to_core(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn on_file_change_request_approval_response(
    event_turn_id: String,
    conversation_id: ThreadId,
    item_id: String,
    changes: Vec<FileUpdateChange>,
    pending_request: PendingServerRequest,
    conversation: Arc<PraxisThread>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state: Arc<Mutex<ThreadState>>,
    permission_guard: ThreadWatchActiveGuard,
) {
    let response = pending_request
        .await_response_and_resolve(&thread_state, permission_guard)
        .await;
    let Some((decision, completion_status)) = file_change_approval_response_outcome(response)
    else {
        return;
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

    if let Err(err) = conversation
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
pub(super) async fn on_command_execution_request_approval_response(
    event_turn_id: String,
    conversation_id: ThreadId,
    approval_id: Option<String>,
    item_id: String,
    completion_item: Option<CommandExecutionCompletionItem>,
    pending_request: PendingServerRequest,
    conversation: Arc<PraxisThread>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state: Arc<Mutex<ThreadState>>,
    permission_guard: ThreadWatchActiveGuard,
) {
    let response = pending_request
        .await_response_and_resolve(&thread_state, permission_guard)
        .await;
    let Some((decision, completion_status)) = command_execution_approval_response_outcome(response)
    else {
        return;
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
