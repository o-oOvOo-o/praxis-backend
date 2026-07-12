use super::*;

pub(super) async fn handle_apply_patch_approval_request(
    event: ApplyPatchApprovalRequestEvent,
    event_turn_id: String,
    conversation_id: ThreadId,
    conversation: Arc<PraxisThread>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state_manager: &ThreadStateManager,
    thread_state: Arc<Mutex<ThreadState>>,
    thread_watch_manager: &ThreadWatchManager,
) {
    let ApplyPatchApprovalRequestEvent {
        call_id,
        turn_id,
        changes,
        reason,
        grant_root,
    } = event;
    let permission_guard = thread_watch_manager
        .note_permission_requested(&conversation_id.to_string())
        .await;
    // Until core emits a first-class FileChangeItem, the call_id is the item_id.
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
        let item_sink =
            ThreadItemNotificationSink::new(&outgoing, &conversation_id, &event_turn_id);
        let item = ThreadItem::FileChange {
            id: item_id.clone(),
            changes: patch_changes.clone(),
            status: PatchApplyStatus::InProgress,
        };
        item_sink.item_started(item).await;
    }

    let params = FileChangeRequestApprovalParams {
        thread_id: conversation_id.to_string(),
        turn_id: turn_id.clone(),
        item_id: item_id.clone(),
        reason,
        grant_root,
    };
    let pending_request = send_server_request(
        thread_state_manager,
        &thread_state,
        &outgoing,
        turn_id.as_str(),
        ServerRequestPayload::FileChangeRequestApproval(params),
    )
    .await;
    tokio::spawn(async move {
        on_file_change_request_approval_response(
            event_turn_id,
            conversation_id,
            item_id,
            patch_changes,
            pending_request,
            conversation,
            outgoing,
            thread_state.clone(),
            permission_guard,
        )
        .await;
    });
}

pub(super) async fn handle_exec_approval_request(
    event: ExecApprovalRequestEvent,
    event_turn_id: String,
    conversation_id: ThreadId,
    conversation: Arc<PraxisThread>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state_manager: &ThreadStateManager,
    thread_state: Arc<Mutex<ThreadState>>,
    thread_watch_manager: &ThreadWatchManager,
) {
    let permission_guard = thread_watch_manager
        .note_permission_requested(&conversation_id.to_string())
        .await;
    let available_decisions = event
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
    } = event;
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
    let proposed_network_policy_amendments = proposed_network_policy_amendments.map(|amendments| {
        amendments
            .into_iter()
            .map(ApiNetworkPolicyAmendment::from)
            .collect()
    });
    let additional_permissions = additional_permissions.map(ApiAdditionalPermissionProfile::from);

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
    let pending_request = send_server_request(
        thread_state_manager,
        &thread_state,
        &outgoing,
        turn_id.as_str(),
        ServerRequestPayload::CommandExecutionRequestApproval(params),
    )
    .await;
    tokio::spawn(async move {
        on_command_execution_request_approval_response(
            event_turn_id,
            conversation_id,
            approval_id,
            call_id,
            completion_item,
            pending_request,
            conversation,
            outgoing,
            thread_state.clone(),
            permission_guard,
        )
        .await;
    });
}
