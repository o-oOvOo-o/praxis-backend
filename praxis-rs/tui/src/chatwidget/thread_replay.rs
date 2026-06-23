use super::*;

impl ChatWidget {
    pub(crate) fn replay_thread_turns(&mut self, turns: Vec<Turn>, replay_kind: ReplayKind) {
        if matches!(replay_kind, ReplayKind::ResumeInitialMessages) {
            self.replay_initial_thread_turns_compact(turns);
            return;
        }

        for turn in turns {
            let Turn {
                id: turn_id,
                items,
                status,
                error,
            } = turn;
            if matches!(status, TurnStatus::InProgress)
                && replay_kind.preserves_live_running_state()
            {
                self.last_non_retry_error = None;
                self.on_task_started();
            }
            for item in items {
                self.replay_thread_item(item, turn_id.clone(), replay_kind);
            }
            self.handle_replayed_turn_status(turn_id, status, error, replay_kind);
        }
    }

    fn replay_initial_thread_turns_compact(&mut self, turns: Vec<Turn>) {
        let mut projector = ResumeReplayProjector::default();
        for turn in turns {
            let Turn {
                id: turn_id,
                items,
                status,
                error,
            } = turn;
            projector.project_items(items, |cell| self.add_to_history(cell));
            self.handle_replayed_turn_status(
                turn_id,
                status,
                error,
                ReplayKind::ResumeInitialMessages,
            );
        }
        projector.finish(|cell| self.add_to_history(cell));
    }

    fn handle_replayed_turn_status(
        &mut self,
        turn_id: String,
        status: TurnStatus,
        error: Option<TurnError>,
        replay_kind: ReplayKind,
    ) {
        if !matches!(
            status,
            TurnStatus::Completed | TurnStatus::Interrupted | TurnStatus::Failed
        ) {
            return;
        }

        self.handle_turn_completed_notification(
            TurnCompletedNotification {
                thread_id: self.thread_id.map(|id| id.to_string()).unwrap_or_default(),
                turn: Turn {
                    id: turn_id,
                    items: Vec::new(),
                    status,
                    error,
                },
            },
            Some(replay_kind),
        );
    }

    pub(crate) fn replay_thread_item(
        &mut self,
        item: ThreadItem,
        turn_id: String,
        replay_kind: ReplayKind,
    ) {
        self.handle_thread_item(item, turn_id, ThreadItemRenderSource::Replay(replay_kind))
    }

    pub(super) fn handle_thread_item(
        &mut self,
        item: ThreadItem,
        turn_id: String,
        render_source: ThreadItemRenderSource,
    ) {
        let from_replay = render_source.is_replay();
        let replay_kind = render_source.replay_kind();
        if matches!(replay_kind, Some(ReplayKind::ResumeInitialMessages)) {
            ResumeReplayProjector::project_single(item, |cell| self.add_to_history(cell));
            return;
        }
        match item {
            ThreadItem::UserMessage { id, content } => {
                let user_message = praxis_protocol::items::UserMessageItem {
                    id,
                    content: content
                        .into_iter()
                        .map(praxis_app_gateway_protocol::UserInput::into_core)
                        .collect(),
                };
                let event = user_message.to_user_message_event();
                if from_replay {
                    self.on_user_message_event(event);
                } else {
                    let rendered = Self::rendered_user_message_event_from_event(&event);
                    let compare_key =
                        Self::pending_steer_compare_key_from_items(&user_message.content);
                    if self
                        .pending_steers
                        .front()
                        .is_some_and(|pending| pending.compare_key == compare_key)
                    {
                        if let Some(pending) = self.pending_steers.pop_front() {
                            self.refresh_pending_input_preview();
                            let pending_event = UserMessageEvent {
                                message: pending.user_message.text,
                                images: Some(pending.user_message.remote_image_urls),
                                local_images: pending
                                    .user_message
                                    .local_images
                                    .into_iter()
                                    .map(|image| image.path)
                                    .collect(),
                                text_elements: pending.user_message.text_elements,
                            };
                            self.on_user_message_event(pending_event);
                        } else if self.last_rendered_user_message_event.as_ref() != Some(&rendered)
                        {
                            tracing::warn!(
                                "pending steer matched compare key but queue was empty when rendering committed user message"
                            );
                            self.on_user_message_event(event);
                        }
                    } else if self.last_rendered_user_message_event.as_ref() != Some(&rendered) {
                        self.on_user_message_event(event);
                    }
                }
            }
            ThreadItem::AgentMessage {
                id,
                text,
                phase,
                memory_citation,
            } => {
                self.on_agent_message_item_completed(AgentMessageItem {
                    id,
                    content: vec![AgentMessageContent::Text { text }],
                    phase,
                    memory_citation: memory_citation.map(|citation| {
                        praxis_protocol::memory_citation::MemoryCitation {
                            entries: citation
                                .entries
                                .into_iter()
                                .map(|entry| {
                                    praxis_protocol::memory_citation::MemoryCitationEntry {
                                        path: entry.path,
                                        line_start: entry.line_start,
                                        line_end: entry.line_end,
                                        note: entry.note,
                                    }
                                })
                                .collect(),
                            rollout_ids: citation.thread_ids,
                        }
                    }),
                });
            }
            ThreadItem::Plan { text, .. } => self.on_plan_item_completed(text),
            ThreadItem::Reasoning {
                summary, content, ..
            } => {
                if from_replay {
                    let has_summary = !summary.is_empty();
                    if has_summary {
                        for delta in summary {
                            self.on_agent_reasoning_delta(delta, ReasoningBlockKind::Summary);
                        }
                        self.on_agent_reasoning_final();
                    }
                    if self.config.show_raw_agent_reasoning || !has_summary {
                        for delta in content {
                            self.on_agent_reasoning_delta(delta, ReasoningBlockKind::Full);
                        }
                        self.on_agent_reasoning_final();
                    }
                } else {
                    self.on_agent_reasoning_final();
                }
            }
            ThreadItem::CommandExecution {
                id,
                command,
                cwd,
                process_id,
                source,
                status,
                command_actions,
                aggregated_output,
                exit_code,
                duration_ms,
            } => {
                if matches!(
                    status,
                    praxis_app_gateway_protocol::CommandExecutionStatus::InProgress
                ) {
                    self.on_exec_command_begin(ExecCommandBeginEvent {
                        call_id: id,
                        process_id,
                        turn_id: turn_id.clone(),
                        command: split_command_string(&command),
                        cwd,
                        parsed_cmd: command_actions
                            .into_iter()
                            .map(praxis_app_gateway_protocol::CommandAction::into_core)
                            .collect(),
                        source: source.to_core(),
                        interaction_input: None,
                    });
                } else {
                    let aggregated_output = aggregated_output.unwrap_or_default();
                    self.on_exec_command_end(ExecCommandEndEvent {
                        call_id: id,
                        process_id,
                        turn_id: turn_id.clone(),
                        command: split_command_string(&command),
                        cwd,
                        parsed_cmd: command_actions
                            .into_iter()
                            .map(praxis_app_gateway_protocol::CommandAction::into_core)
                            .collect(),
                        source: source.to_core(),
                        interaction_input: None,
                        stdout: String::new(),
                        stderr: String::new(),
                        aggregated_output: aggregated_output.clone(),
                        exit_code: exit_code.unwrap_or_default(),
                        duration: Duration::from_millis(
                            duration_ms.unwrap_or_default().max(0) as u64
                        ),
                        formatted_output: aggregated_output,
                        status: match status {
                            praxis_app_gateway_protocol::CommandExecutionStatus::Completed => {
                                praxis_protocol::protocol::ExecCommandStatus::Completed
                            }
                            praxis_app_gateway_protocol::CommandExecutionStatus::Failed => {
                                praxis_protocol::protocol::ExecCommandStatus::Failed
                            }
                            praxis_app_gateway_protocol::CommandExecutionStatus::Declined => {
                                praxis_protocol::protocol::ExecCommandStatus::Declined
                            }
                            praxis_app_gateway_protocol::CommandExecutionStatus::InProgress => {
                                praxis_protocol::protocol::ExecCommandStatus::Failed
                            }
                        },
                    });
                }
            }
            ThreadItem::FileChange {
                id,
                changes,
                status,
            } => {
                if !matches!(
                    status,
                    praxis_app_gateway_protocol::PatchApplyStatus::InProgress
                ) {
                    self.on_patch_apply_end(praxis_protocol::protocol::PatchApplyEndEvent {
                        call_id: id,
                        turn_id: turn_id.clone(),
                        stdout: String::new(),
                        stderr: String::new(),
                        success: !matches!(
                            status,
                            praxis_app_gateway_protocol::PatchApplyStatus::Failed
                        ),
                        changes: app_gateway_patch_changes_to_core(changes),
                        status: match status {
                            praxis_app_gateway_protocol::PatchApplyStatus::Completed => {
                                praxis_protocol::protocol::PatchApplyStatus::Completed
                            }
                            praxis_app_gateway_protocol::PatchApplyStatus::Failed => {
                                praxis_protocol::protocol::PatchApplyStatus::Failed
                            }
                            praxis_app_gateway_protocol::PatchApplyStatus::Declined => {
                                praxis_protocol::protocol::PatchApplyStatus::Declined
                            }
                            praxis_app_gateway_protocol::PatchApplyStatus::InProgress => {
                                praxis_protocol::protocol::PatchApplyStatus::Failed
                            }
                        },
                    });
                }
            }
            ThreadItem::McpToolCall {
                id,
                server,
                tool,
                arguments,
                result,
                error,
                duration_ms,
                ..
            } => {
                self.on_mcp_tool_call_end(praxis_protocol::protocol::McpToolCallEndEvent {
                    call_id: id,
                    invocation: praxis_protocol::protocol::McpInvocation {
                        server,
                        tool,
                        arguments: Some(arguments),
                    },
                    duration: Duration::from_millis(duration_ms.unwrap_or_default().max(0) as u64),
                    result: match (result, error) {
                        (_, Some(error)) => Err(error.message),
                        (Some(result), None) => Ok(praxis_protocol::mcp::CallToolResult {
                            content: result.content,
                            structured_content: result.structured_content,
                            is_error: Some(false),
                            meta: None,
                        }),
                        (None, None) => Err("MCP tool call completed without a result".to_string()),
                    },
                });
            }
            ThreadItem::WebSearch { id, query, action } => {
                self.on_web_search_begin(WebSearchBeginEvent {
                    call_id: id.clone(),
                });
                self.on_web_search_end(WebSearchEndEvent {
                    call_id: id,
                    query,
                    action: action
                        .map(app_gateway_web_search_action_to_core)
                        .unwrap_or(praxis_protocol::models::WebSearchAction::Other),
                });
            }
            ThreadItem::ImageView { id, path } => {
                self.on_view_image_tool_call(ViewImageToolCallEvent {
                    call_id: id,
                    path: path.into(),
                });
            }
            ThreadItem::ImageGeneration {
                id,
                status,
                revised_prompt,
                result,
                saved_path,
            } => {
                self.on_image_generation_end(ImageGenerationEndEvent {
                    call_id: id,
                    result,
                    revised_prompt,
                    status,
                    saved_path,
                });
            }
            ThreadItem::EnteredReviewMode { review, .. } => {
                if from_replay {
                    self.enter_review_mode_with_hint(review, /*from_replay*/ true);
                }
            }
            ThreadItem::ExitedReviewMode { .. } => {
                self.exit_review_mode_after_item();
            }
            ThreadItem::ContextCompaction { .. } => {
                self.on_agent_message("Context compacted".to_owned());
            }
            ThreadItem::HookPrompt { .. } => {}
            ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                model,
                reasoning_effort,
                agents_states,
            } => self.on_collab_agent_tool_call(ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                model,
                reasoning_effort,
                agents_states,
            }),
            ThreadItem::DynamicToolCall { .. } => {}
        }

        if matches!(replay_kind, Some(ReplayKind::ThreadSnapshot)) && turn_id.is_empty() {
            self.request_redraw();
        }
    }
}
