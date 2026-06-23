use super::*;

impl ChatWidget {
    pub(super) fn finalize_completed_assistant_message(&mut self, message: Option<&str>) {
        // If we have a stream_controller, the finalized message payload is redundant because the
        // visible content has already been accumulated through deltas.
        if self.stream_controller.is_none()
            && let Some(message) = message
            && !message.is_empty()
        {
            self.handle_streaming_delta(message.to_string());
        }
        self.flush_answer_stream_with_separator();
        self.handle_stream_finished();
        self.request_redraw();
    }

    pub(super) fn on_agent_message(&mut self, message: String) {
        self.finalize_completed_assistant_message(Some(&message));
    }

    pub(super) fn on_agent_message_delta(&mut self, delta: String) {
        self.handle_streaming_delta(delta);
    }

    pub(super) fn on_plan_delta(&mut self, delta: String) {
        if self.active_mode_kind() != ModeKind::Plan {
            return;
        }
        if !self.plan_item_active {
            self.plan_item_active = true;
            self.plan_delta_buffer.clear();
        }
        self.plan_delta_buffer.push_str(&delta);
        // Before streaming plan content, flush any active exec cell group.
        self.flush_unified_exec_wait_streak();
        self.flush_active_cell();

        if self.plan_stream_controller.is_none() {
            self.plan_stream_controller = Some(PlanStreamController::new(
                self.last_rendered_width.get().map(|w| w.saturating_sub(4)),
                &self.config.cwd,
            ));
        }
        if let Some(controller) = self.plan_stream_controller.as_mut()
            && controller.push(&delta)
        {
            self.app_event_tx.send(AppEvent::StartCommitAnimation);
            self.run_catch_up_commit_tick();
        }
        self.request_redraw();
    }

    pub(super) fn on_plan_item_completed(&mut self, text: String) {
        let streamed_plan = self.plan_delta_buffer.trim().to_string();
        let plan_text = if text.trim().is_empty() {
            streamed_plan
        } else {
            text
        };
        if !plan_text.trim().is_empty() {
            self.last_copyable_output = Some(plan_text.clone());
        }
        // Plan commit ticks can hide the status row; remember whether we streamed plan output so
        // completion can restore it once stream queues are idle.
        let should_restore_after_stream = self.plan_stream_controller.is_some();
        self.plan_delta_buffer.clear();
        self.plan_item_active = false;
        self.saw_plan_item_this_turn = true;
        let finalized_streamed_cell =
            if let Some(mut controller) = self.plan_stream_controller.take() {
                controller.finalize()
            } else {
                None
            };
        if let Some(cell) = finalized_streamed_cell {
            self.add_boxed_history(cell);
            // TODO: Replace streamed output with the final plan item text if plan streaming is
            // removed or if we need to reconcile mismatches between streamed and final content.
        } else if !plan_text.is_empty() {
            self.add_to_history(history_cell::new_proposed_plan(plan_text, &self.config.cwd));
        }
        if should_restore_after_stream {
            self.pending_status_indicator_restore = true;
            self.maybe_restore_status_indicator_after_stream_idle();
        }
    }

    pub(super) fn on_agent_reasoning_delta(&mut self, delta: String, kind: ReasoningBlockKind) {
        if self.reasoning_block_kind.is_none() || kind == ReasoningBlockKind::Full {
            self.reasoning_block_kind = Some(kind);
        }
        self.reasoning_buffer.push_str(&delta);

        if self.unified_exec_wait_streak.is_some() {
            // Unified exec waiting should take precedence over reasoning-derived status headers.
            self.request_redraw();
            return;
        }

        self.terminal_title_status_kind = TerminalTitleStatusKind::Reasoning;
        let header = extract_first_bold(&self.reasoning_buffer).unwrap_or_else(|| match kind {
            ReasoningBlockKind::Summary => "Reasoning summary".to_string(),
            ReasoningBlockKind::Full => "Reasoning".to_string(),
        });
        let thinking_persona = self.thinking_persona_for_reasoning_kind(kind);
        let preview_max_lines = match kind {
            ReasoningBlockKind::Summary => REASONING_SUMMARY_STATUS_PREVIEW_MAX_LINES,
            ReasoningBlockKind::Full => REASONING_FULL_STATUS_PREVIEW_MAX_LINES,
        };
        if let Some(preview) = reasoning_status_preview(&self.reasoning_buffer, preview_max_lines) {
            self.set_status_with_thinking_persona(
                header,
                Some(preview),
                StatusDetailsCapitalization::Preserve,
                preview_max_lines,
                thinking_persona,
            );
        } else {
            self.set_status_with_thinking_persona(
                header,
                /*details*/ None,
                StatusDetailsCapitalization::CapitalizeFirst,
                STATUS_DETAILS_DEFAULT_MAX_LINES,
                thinking_persona,
            );
        }
        self.request_redraw();
    }

    pub(super) fn thinking_persona_for_reasoning_kind(
        &self,
        kind: ReasoningBlockKind,
    ) -> ThinkingPersona {
        match kind {
            ReasoningBlockKind::Summary => ThinkingPersona::PraxisSummary,
            ReasoningBlockKind::Full if self.is_deepseek_current_model() => {
                ThinkingPersona::DeepSeekFull
            }
            ReasoningBlockKind::Full => ThinkingPersona::None,
        }
    }

    pub(super) fn is_deepseek_current_model(&self) -> bool {
        self.current_model()
            .to_ascii_lowercase()
            .contains("deepseek")
            || self
                .current_model_provider_id()
                .to_ascii_lowercase()
                .contains("deepseek")
    }

    pub(super) fn on_agent_reasoning_final(&mut self) {
        // At the end of a reasoning block, record transcript-only content.
        self.full_reasoning_buffer.push_str(&self.reasoning_buffer);
        if !self.full_reasoning_buffer.is_empty() {
            let kind = self
                .reasoning_block_kind
                .take()
                .unwrap_or(ReasoningBlockKind::Summary);
            let cell = match kind {
                ReasoningBlockKind::Summary => history_cell::new_reasoning_summary_block(
                    self.full_reasoning_buffer.clone(),
                    &self.config.cwd,
                ),
                ReasoningBlockKind::Full => history_cell::new_reasoning_full_block(
                    self.full_reasoning_buffer.clone(),
                    &self.config.cwd,
                ),
            };
            self.add_boxed_history(cell);
        }
        self.reasoning_buffer.clear();
        self.full_reasoning_buffer.clear();
        self.reasoning_block_kind = None;
        self.pending_goal_completion_elapsed = None;
        self.request_redraw();
    }

    pub(super) fn on_reasoning_section_break(&mut self) {
        // Start a new reasoning block for header extraction and accumulate transcript.
        self.full_reasoning_buffer.push_str(&self.reasoning_buffer);
        self.full_reasoning_buffer.push_str("\n\n");
        self.reasoning_buffer.clear();
    }

    // Raw reasoning uses the same flow as summarized reasoning
}
