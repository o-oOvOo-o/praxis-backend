use super::*;

impl ThreadHistoryBuilder {
    pub(super) fn handle_response_item(&mut self, item: &praxis_protocol::models::ResponseItem) {
        let praxis_protocol::models::ResponseItem::Message {
            role,
            content,
            id,
            phase,
            ..
        } = item
        else {
            return;
        };

        match role.as_str() {
            "user" => {
                let Some(hook_prompt) = parse_hook_prompt_message(id.as_ref(), content) else {
                    return;
                };

                self.ensure_turn().items.push(ThreadItem::HookPrompt {
                    id: hook_prompt.id,
                    fragments: hook_prompt
                        .fragments
                        .into_iter()
                        .map(crate::protocol::api::HookPromptFragment::from)
                        .collect(),
                });
            }
            "assistant" => {
                let text = content
                    .iter()
                    .filter_map(|item| match item {
                        praxis_protocol::models::ContentItem::InputText { text }
                        | praxis_protocol::models::ContentItem::OutputText { text } => Some(text),
                        praxis_protocol::models::ContentItem::InputImage { .. } => None,
                    })
                    .cloned()
                    .collect::<String>();
                self.handle_agent_message(text, phase.clone(), None);
            }
            _ => {}
        }
    }

    pub(super) fn handle_user_message(&mut self, payload: &UserMessageEvent) {
        // User messages should stay in explicitly opened turns. For backward
        // compatibility with older streams that did not open turns explicitly,
        // close any implicit/inactive turn and start a fresh one for this input.
        if let Some(turn) = self.current_turn.as_ref()
            && !turn.opened_explicitly
            && !(turn.saw_compaction && turn.items.is_empty())
        {
            self.finish_current_turn();
        }
        let mut turn = self
            .current_turn
            .take()
            .unwrap_or_else(|| self.new_turn(/*id*/ None));
        let id = self.next_item_id();
        let content = self.build_user_inputs(payload);
        turn.items.push(ThreadItem::UserMessage { id, content });
        self.current_turn = Some(turn);
    }

    pub(super) fn handle_agent_message(
        &mut self,
        text: String,
        phase: Option<MessagePhase>,
        memory_citation: Option<crate::protocol::api::MemoryCitation>,
    ) {
        if text.is_empty() {
            return;
        }

        if matches!(
            self.ensure_turn().items.last(),
            Some(ThreadItem::AgentMessage {
                text: existing_text,
                phase: existing_phase,
                memory_citation: existing_citation,
                ..
            }) if existing_text == &text
                && existing_phase == &phase
                && existing_citation == &memory_citation
        ) {
            return;
        }

        let id = self.next_item_id();
        self.ensure_turn().items.push(ThreadItem::AgentMessage {
            id,
            text,
            phase,
            memory_citation,
        });
    }

    pub(super) fn handle_agent_reasoning(&mut self, payload: &AgentReasoningEvent) {
        if payload.text.is_empty() {
            return;
        }

        // If the last item is a reasoning item, add the new text to the summary.
        if let Some(ThreadItem::Reasoning { summary, .. }) = self.ensure_turn().items.last_mut() {
            summary.push(payload.text.clone());
            return;
        }

        // Otherwise, create a new reasoning item.
        let id = self.next_item_id();
        self.ensure_turn().items.push(ThreadItem::Reasoning {
            id,
            summary: vec![payload.text.clone()],
            content: Vec::new(),
        });
    }

    pub(super) fn handle_agent_reasoning_raw_content(
        &mut self,
        payload: &AgentReasoningRawContentEvent,
    ) {
        if payload.text.is_empty() {
            return;
        }

        // If the last item is a reasoning item, add the new text to the content.
        if let Some(ThreadItem::Reasoning { content, .. }) = self.ensure_turn().items.last_mut() {
            content.push(payload.text.clone());
            return;
        }

        // Otherwise, create a new reasoning item.
        let id = self.next_item_id();
        self.ensure_turn().items.push(ThreadItem::Reasoning {
            id,
            summary: Vec::new(),
            content: vec![payload.text.clone()],
        });
    }

    pub(super) fn handle_item_started(&mut self, payload: &ItemStartedEvent) {
        match &payload.item {
            praxis_protocol::items::TurnItem::Plan(plan) => {
                if plan.text.is_empty() {
                    return;
                }
                self.upsert_item_in_turn_id(
                    &payload.turn_id,
                    ThreadItem::from(payload.item.clone()),
                );
            }
            praxis_protocol::items::TurnItem::UserMessage(_)
            | praxis_protocol::items::TurnItem::HookPrompt(_)
            | praxis_protocol::items::TurnItem::AgentMessage(_)
            | praxis_protocol::items::TurnItem::Reasoning(_)
            | praxis_protocol::items::TurnItem::WebSearch(_)
            | praxis_protocol::items::TurnItem::ImageGeneration(_)
            | praxis_protocol::items::TurnItem::ContextCompaction(_) => {}
        }
    }

    pub(super) fn handle_item_completed(&mut self, payload: &ItemCompletedEvent) {
        match &payload.item {
            praxis_protocol::items::TurnItem::UserMessage(_) => {
                self.record_canonical_user_message(
                    payload.turn_id.as_str(),
                    ThreadItem::from(payload.item.clone()),
                );
            }
            praxis_protocol::items::TurnItem::Plan(plan) => {
                if plan.text.is_empty() {
                    return;
                }
                self.upsert_item_in_turn_id(
                    &payload.turn_id,
                    ThreadItem::from(payload.item.clone()),
                );
            }
            praxis_protocol::items::TurnItem::HookPrompt(_)
            | praxis_protocol::items::TurnItem::AgentMessage(_)
            | praxis_protocol::items::TurnItem::Reasoning(_)
            | praxis_protocol::items::TurnItem::WebSearch(_)
            | praxis_protocol::items::TurnItem::ImageGeneration(_)
            | praxis_protocol::items::TurnItem::ContextCompaction(_) => {}
        }
    }

    pub(super) fn handle_web_search_begin(&mut self, payload: &WebSearchBeginEvent) {
        let item = ThreadItem::WebSearch {
            id: payload.call_id.clone(),
            query: String::new(),
            action: None,
        };
        self.upsert_item_in_current_turn(item);
    }

    pub(super) fn handle_web_search_end(&mut self, payload: &WebSearchEndEvent) {
        let item = ThreadItem::WebSearch {
            id: payload.call_id.clone(),
            query: payload.query.clone(),
            action: Some(WebSearchAction::from(payload.action.clone())),
        };
        self.upsert_item_in_current_turn(item);
    }

    pub(super) fn handle_exec_command_begin(&mut self, payload: &ExecCommandBeginEvent) {
        let command = shlex::try_join(payload.command.iter().map(String::as_str))
            .unwrap_or_else(|_| payload.command.join(" "));
        let command_actions = payload
            .parsed_cmd
            .iter()
            .cloned()
            .map(CommandAction::from)
            .collect();
        let item = ThreadItem::CommandExecution {
            id: payload.call_id.clone(),
            command,
            cwd: payload.cwd.clone(),
            process_id: payload.process_id.clone(),
            source: payload.source.into(),
            status: CommandExecutionStatus::InProgress,
            command_actions,
            aggregated_output: None,
            exit_code: None,
            duration_ms: None,
        };
        self.upsert_item_in_turn_id(&payload.turn_id, item);
    }

    pub(super) fn handle_exec_command_end(&mut self, payload: &ExecCommandEndEvent) {
        let status: CommandExecutionStatus = (&payload.status).into();
        let duration_ms = i64::try_from(payload.duration.as_millis()).unwrap_or(i64::MAX);
        let aggregated_output = if payload.aggregated_output.is_empty() {
            None
        } else {
            Some(payload.aggregated_output.clone())
        };
        let command = shlex::try_join(payload.command.iter().map(String::as_str))
            .unwrap_or_else(|_| payload.command.join(" "));
        let command_actions = payload
            .parsed_cmd
            .iter()
            .cloned()
            .map(CommandAction::from)
            .collect();
        let item = ThreadItem::CommandExecution {
            id: payload.call_id.clone(),
            command,
            cwd: payload.cwd.clone(),
            process_id: payload.process_id.clone(),
            source: payload.source.into(),
            status,
            command_actions,
            aggregated_output,
            exit_code: Some(payload.exit_code),
            duration_ms: Some(duration_ms),
        };
        // Command completions can arrive out of order. Unified exec may return
        // while a PTY is still running, then emit ExecCommandEnd later from a
        // background exit watcher when that process finally exits. By then, a
        // newer user turn may already have started. Route by event turn_id so
        // replay preserves the original turn association.
        self.upsert_item_in_turn_id(&payload.turn_id, item);
    }

    pub(super) fn handle_apply_patch_approval_request(
        &mut self,
        payload: &ApplyPatchApprovalRequestEvent,
    ) {
        let item = ThreadItem::FileChange {
            id: payload.call_id.clone(),
            changes: convert_patch_changes(&payload.changes),
            status: PatchApplyStatus::InProgress,
        };
        if payload.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&payload.turn_id, item);
        }
    }

    pub(super) fn handle_patch_apply_begin(&mut self, payload: &PatchApplyBeginEvent) {
        let item = ThreadItem::FileChange {
            id: payload.call_id.clone(),
            changes: convert_patch_changes(&payload.changes),
            status: PatchApplyStatus::InProgress,
        };
        if payload.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&payload.turn_id, item);
        }
    }

    pub(super) fn handle_patch_apply_end(&mut self, payload: &PatchApplyEndEvent) {
        let status: PatchApplyStatus = (&payload.status).into();
        let item = ThreadItem::FileChange {
            id: payload.call_id.clone(),
            changes: convert_patch_changes(&payload.changes),
            status,
        };
        if payload.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&payload.turn_id, item);
        }
    }

    pub(super) fn handle_dynamic_tool_call_request(
        &mut self,
        payload: &praxis_protocol::dynamic_tools::DynamicToolCallRequest,
    ) {
        let item = ThreadItem::DynamicToolCall {
            id: payload.call_id.clone(),
            tool: payload.tool.clone(),
            arguments: payload.arguments.clone(),
            status: DynamicToolCallStatus::InProgress,
            content_items: None,
            success: None,
            duration_ms: None,
        };
        if payload.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&payload.turn_id, item);
        }
    }

    pub(super) fn handle_dynamic_tool_call_response(
        &mut self,
        payload: &DynamicToolCallResponseEvent,
    ) {
        let status = if payload.success {
            DynamicToolCallStatus::Completed
        } else {
            DynamicToolCallStatus::Failed
        };
        let duration_ms = i64::try_from(payload.duration.as_millis()).ok();
        let item = ThreadItem::DynamicToolCall {
            id: payload.call_id.clone(),
            tool: payload.tool.clone(),
            arguments: payload.arguments.clone(),
            status,
            content_items: Some(convert_dynamic_tool_content_items(&payload.content_items)),
            success: Some(payload.success),
            duration_ms,
        };
        if payload.turn_id.is_empty() {
            self.upsert_item_in_current_turn(item);
        } else {
            self.upsert_item_in_turn_id(&payload.turn_id, item);
        }
    }

    pub(super) fn handle_mcp_tool_call_begin(&mut self, payload: &McpToolCallBeginEvent) {
        let item = ThreadItem::McpToolCall {
            id: payload.call_id.clone(),
            server: payload.invocation.server.clone(),
            tool: payload.invocation.tool.clone(),
            status: McpToolCallStatus::InProgress,
            arguments: payload
                .invocation
                .arguments
                .clone()
                .unwrap_or(serde_json::Value::Null),
            result: None,
            error: None,
            duration_ms: None,
        };
        self.upsert_item_in_current_turn(item);
    }

    pub(super) fn handle_mcp_tool_call_end(&mut self, payload: &McpToolCallEndEvent) {
        let status = if payload.is_success() {
            McpToolCallStatus::Completed
        } else {
            McpToolCallStatus::Failed
        };
        let duration_ms = i64::try_from(payload.duration.as_millis()).ok();
        let (result, error) = match &payload.result {
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
            id: payload.call_id.clone(),
            server: payload.invocation.server.clone(),
            tool: payload.invocation.tool.clone(),
            status,
            arguments: payload
                .invocation
                .arguments
                .clone()
                .unwrap_or(serde_json::Value::Null),
            result,
            error,
            duration_ms,
        };
        self.upsert_item_in_current_turn(item);
    }

    pub(super) fn handle_view_image_tool_call(&mut self, payload: &ViewImageToolCallEvent) {
        let item = ThreadItem::ImageView {
            id: payload.call_id.clone(),
            path: payload.path.to_string_lossy().into_owned(),
        };
        self.upsert_item_in_current_turn(item);
    }

    pub(super) fn handle_image_generation_begin(&mut self, payload: &ImageGenerationBeginEvent) {
        let item = ThreadItem::ImageGeneration {
            id: payload.call_id.clone(),
            status: String::new(),
            revised_prompt: None,
            result: String::new(),
            saved_path: None,
        };
        self.upsert_item_in_current_turn(item);
    }

    pub(super) fn handle_image_generation_end(&mut self, payload: &ImageGenerationEndEvent) {
        let item = ThreadItem::ImageGeneration {
            id: payload.call_id.clone(),
            status: payload.status.clone(),
            revised_prompt: payload.revised_prompt.clone(),
            result: payload.result.clone(),
            saved_path: payload.saved_path.clone(),
        };
        self.upsert_item_in_current_turn(item);
    }
}
