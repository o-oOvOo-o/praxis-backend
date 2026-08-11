use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_bespoke_event_handling(
    event: Event,
    conversation_id: ThreadId,
    conversation: Arc<PraxisThread>,
    thread_manager: Arc<ThreadManager>,
    outgoing: ThreadScopedOutgoingMessageSender,
    thread_state_manager: ThreadStateManager,
    thread_state: Arc<tokio::sync::Mutex<ThreadState>>,
    thread_watch_manager: ThreadWatchManager,
    workspace_change_store: WorkspaceChangeStore,
    fallback_model_provider: String,
    praxis_home: &Path,
    state_db: Option<Arc<StateRuntime>>,
) {
    let Event {
        id: event_turn_id,
        msg,
    } = event;
    let item_sink = ThreadItemNotificationSink::new(&outgoing, &conversation_id, &event_turn_id);
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
                        collaboration_mode_kind: payload.collaboration_mode_kind,
                        items: Vec::new(),
                        error: None,
                        status: TurnStatus::InProgress,
                    })
                };
                let notification = TurnStartedNotification {
                    thread_id: conversation_id.to_string(),
                    turn,
                    model_context_window: payload.model_context_window,
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
            let turn_id = event_turn_id.clone();
            let (status, error) =
                handle_turn_complete(conversation_id, event_turn_id, &outgoing, &thread_state)
                    .await;
            finish_automation_runs_for_turn(
                state_db.as_ref(),
                &conversation_id,
                turn_id.as_str(),
                &status,
                error.as_ref(),
                &outgoing,
            )
            .await;
            advance_selfwork_after_turn(
                conversation_id,
                turn_id.as_str(),
                &status,
                &conversation,
                &thread_state,
                &outgoing,
                state_db.as_ref(),
            )
            .await;
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
            send_realtime_started(&outgoing, &conversation_id, event).await;
        }
        EventMsg::RealtimeConversationRealtime(event) => {
            send_realtime_event(&outgoing, &conversation_id, event.payload).await;
        }
        EventMsg::RealtimeConversationClosed(event) => {
            send_realtime_closed(&outgoing, &conversation_id, event).await;
        }
        EventMsg::ApplyPatchApprovalRequest(event) => {
            handle_apply_patch_approval_request(
                event,
                event_turn_id,
                conversation_id,
                conversation,
                outgoing,
                &thread_state_manager,
                thread_state,
                &thread_watch_manager,
            )
            .await;
        }
        EventMsg::ExecApprovalRequest(event) => {
            handle_exec_approval_request(
                event,
                event_turn_id,
                conversation_id,
                conversation,
                outgoing,
                &thread_state_manager,
                thread_state,
                &thread_watch_manager,
            )
            .await;
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
                .collect::<Vec<_>>();
            if conversation.is_promptless_full_access()
                && praxis_app_gateway_protocol::is_mcp_tool_approval_user_input(&questions)
            {
                return;
            }
            let params = ToolRequestUserInputParams {
                thread_id: conversation_id.to_string(),
                turn_id: request.turn_id,
                item_id: request.call_id,
                questions,
            };
            let pending_request = send_server_request(
                &thread_state_manager,
                &thread_state,
                &outgoing,
                event_turn_id.as_str(),
                ServerRequestPayload::ToolRequestUserInput(params),
            )
            .await;
            tokio::spawn(async move {
                on_request_user_input_response(
                    event_turn_id,
                    pending_request,
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
            if conversation.is_promptless_full_access()
                && praxis_app_gateway_protocol::is_mcp_tool_approval_elicitation(&request_body)
            {
                return;
            }
            let params = McpServerElicitationRequestParams {
                thread_id: conversation_id.to_string(),
                turn_id,
                server_name: request.server_name.clone(),
                request: request_body,
            };
            let pending_request = send_server_request(
                &thread_state_manager,
                &thread_state,
                &outgoing,
                event_turn_id.as_str(),
                ServerRequestPayload::McpServerElicitationRequest(params),
            )
            .await;
            tokio::spawn(async move {
                on_mcp_server_elicitation_response(
                    request.server_name,
                    request.id,
                    pending_request,
                    conversation,
                    thread_state,
                    permission_guard,
                )
                .await;
            });
        }
        EventMsg::RequestPermissions(request) => {
            if conversation.is_promptless_full_access() {
                return;
            }
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
            let pending_request = send_server_request(
                &thread_state_manager,
                &thread_state,
                &outgoing,
                event_turn_id.as_str(),
                ServerRequestPayload::PermissionsRequestApproval(params),
            )
            .await;
            tokio::spawn(async move {
                on_request_permissions_response(
                    request.call_id,
                    requested_permissions,
                    pending_request,
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
            item_sink
                .for_turn_id(turn_id.clone())
                .item_started(item)
                .await;
            let params = DynamicToolCallParams {
                thread_id: conversation_id.to_string(),
                turn_id: turn_id.clone(),
                call_id: call_id.clone(),
                tool: tool.clone(),
                arguments: arguments.clone(),
            };
            let pending_request = send_server_request(
                &thread_state_manager,
                &thread_state,
                &outgoing,
                event_turn_id.as_str(),
                ServerRequestPayload::DynamicToolCall(params),
            )
            .await;
            tokio::spawn(async move {
                crate::dynamic_tools::on_call_response(
                    call_id,
                    pending_request,
                    conversation,
                    thread_state,
                )
                .await;
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
            item_sink
                .for_turn_id(response.turn_id)
                .item_completed(item)
                .await;
        }
        // TODO(celia): properly construct McpToolCall TurnItem in core.
        EventMsg::McpToolCallBegin(begin_event) => {
            let item = construct_mcp_tool_call_item(begin_event);
            item_sink.item_started(item).await;
        }
        EventMsg::McpToolCallEnd(end_event) => {
            let item = construct_mcp_tool_call_end_item(end_event);
            item_sink.item_completed(item).await;
        }
        EventMsg::CollabAgentSpawnBegin(begin_event) => {
            let item = collab_spawn_begin_item(begin_event);
            item_sink.item_started(item).await;
        }
        EventMsg::CollabAgentSpawnEnd(end_event) => {
            let item = collab_spawn_end_item(end_event);
            item_sink.item_completed(item).await;
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
            let item = collab_interaction_begin_item(begin_event);
            item_sink.item_started(item).await;
        }
        EventMsg::CollabAgentInteractionEnd(end_event) => {
            if collab_agent_status_failed(&end_event.status) {
                thread_watch_manager
                    .release_thread_control(&end_event.receiver_thread_id.to_string())
                    .await;
            }
            let item = collab_interaction_end_item(end_event);
            item_sink.item_completed(item).await;
        }
        EventMsg::CollabWaitingBegin(begin_event) => {
            let item = collab_waiting_begin_item(begin_event);
            item_sink.item_started(item).await;
        }
        EventMsg::CollabWaitingEnd(end_event) => {
            let item = collab_waiting_end_item(end_event);
            item_sink.item_completed(item).await;
        }
        EventMsg::CollabCloseBegin(begin_event) => {
            let item = collab_close_begin_item(begin_event);
            item_sink.item_started(item).await;
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
            let item = collab_close_end_item(end_event);
            item_sink.item_completed(item).await;
        }
        EventMsg::CollabResumeBegin(begin_event) => {
            let item = collab_resume_begin_item(begin_event);
            item_sink.item_started(item).await;
        }
        EventMsg::CollabResumeEnd(end_event) => {
            let item = collab_resume_end_item(end_event);
            item_sink.item_completed(item).await;
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
                Some(CorePraxisErrorInfo::ThreadRollbackFailed)
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
                praxis_error_info: ev.praxis_error_info.map(ApiPraxisErrorInfo::from),
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
                praxis_error_info: ev.praxis_error_info.map(ApiPraxisErrorInfo::from),
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
            item_sink.item_started_and_completed(item).await;
        }
        EventMsg::WebSearchBegin(web_search_event) => {
            let item = ThreadItem::WebSearch {
                id: web_search_event.call_id,
                query: String::new(),
                action: None,
            };
            item_sink.item_started(item).await;
        }
        EventMsg::WebSearchEnd(web_search_event) => {
            let item = ThreadItem::WebSearch {
                id: web_search_event.call_id,
                query: web_search_event.query,
                action: Some(praxis_app_gateway_protocol::WebSearchAction::from(
                    web_search_event.action,
                )),
            };
            item_sink.item_completed(item).await;
        }
        EventMsg::EnteredReviewMode(review_request) => {
            let review = review_request
                .user_facing_hint
                .unwrap_or_else(|| review_prompts::user_facing_hint(&review_request.target));
            let item = ThreadItem::EnteredReviewMode {
                id: event_turn_id.clone(),
                review,
            };
            item_sink.item_started_and_completed(item).await;
        }
        EventMsg::ItemStarted(item_started_event) => {
            let item: ThreadItem = item_started_event.item.clone().into();
            item_sink.item_started(item).await;
        }
        EventMsg::ItemCompleted(item_completed_event) => {
            let item: ThreadItem = item_completed_event.item.clone().into();
            item_sink.item_completed(item).await;
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
            item_sink.item_started_and_completed(item).await;
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
                item_sink.item_started(item).await;
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
            item_sink.item_started(item).await;
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

            item_sink.item_completed(item).await;
        }
        // If this is a TurnAborted, reply to any pending interrupt requests.
        EventMsg::TurnAborted(_) => {
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
            let turn_id = event_turn_id.clone();
            let (status, error) =
                handle_turn_interrupted(conversation_id, event_turn_id, &outgoing, &thread_state)
                    .await;
            finish_automation_runs_for_turn(
                state_db.as_ref(),
                &conversation_id,
                turn_id.as_str(),
                &status,
                error.as_ref(),
                &outgoing,
            )
            .await;
            advance_selfwork_after_turn(
                conversation_id,
                turn_id.as_str(),
                &status,
                &conversation,
                &thread_state,
                &outgoing,
                state_db.as_ref(),
            )
            .await;
        }
        EventMsg::ThreadRolledBack(rollback_event) => {
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
                let response = match project_rollback_thread_from_rollout(
                    rollout_path.as_path(),
                    fallback_model_provider.as_str(),
                    praxis_home,
                    &conversation_id,
                )
                .await
                {
                    Ok(mut thread) => {
                        let runtime_state = thread_watch_manager
                            .loaded_runtime_state_for_thread(&thread.id)
                            .await;
                        thread.status = runtime_state.status;
                        thread.control_state = runtime_state.control_state;
                        ThreadRollbackResponse {
                            thread,
                            workspace_restore: rollback_event.workspace_restore.as_ref().map(
                                |outcome| ThreadWorkspaceRestoreResult {
                                    checkpoint_id: outcome.checkpoint_id,
                                    restored_files: outcome.restored_files,
                                    removed_files: outcome.removed_files,
                                },
                            ),
                        }
                    }
                    Err(message) => {
                        let error = JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message,
                            data: None,
                        };
                        outgoing.send_error(request_id, error).await;
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
            let root = conversation.config_snapshot().await.cwd;
            handle_turn_diff(
                conversation_id,
                &event_turn_id,
                turn_diff_event,
                &outgoing,
                root,
                &workspace_change_store,
            )
            .await;
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
