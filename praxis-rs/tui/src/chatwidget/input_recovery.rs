use super::*;

impl ChatWidget {
    /// Handle a turn aborted due to user interrupt.
    ///
    /// Queued user messages are restored into the composer unless the interrupt
    /// was explicitly used to submit pending steer instructions immediately.
    pub(super) fn on_interrupted_turn(&mut self, reason: TurnAbortReason) {
        // Finalize, log a gentle prompt, and clear running state.
        self.finalize_turn();
        let send_pending_steers_immediately = self.submit_pending_steers_after_interrupt;
        self.submit_pending_steers_after_interrupt = false;
        if reason != TurnAbortReason::ReviewEnded {
            if send_pending_steers_immediately {
                self.add_to_history(history_cell::new_info_event(
                    "Model interrupted to submit steer instructions.".to_owned(),
                    /*hint*/ None,
                ));
            } else {
                self.add_to_history(history_cell::new_error_event(
                    "Conversation interrupted - tell the model what to do differently. Something went wrong? Hit `/feedback` to report the issue.".to_owned(),
                ));
            }
        }

        // Core clears pending_input before emitting TurnAborted, so any unacknowledged steers
        // still tracked here must be restored locally instead of waiting for a later commit.
        if send_pending_steers_immediately {
            let pending_steers: Vec<UserMessage> = self
                .pending_steers
                .drain(..)
                .map(|pending| pending.user_message)
                .collect();
            if !pending_steers.is_empty() {
                self.submit_user_message(merge_user_messages(pending_steers));
            } else if let Some(combined) = self.drain_pending_messages_for_restore() {
                self.restore_user_message_to_composer(combined);
            }
        } else if let Some(combined) = self.drain_pending_messages_for_restore() {
            self.restore_user_message_to_composer(combined);
        }
        self.refresh_pending_input_preview();

        self.request_redraw();
    }

    /// Finish a historically replayed aborted turn without showing a live interruption banner.
    pub(super) fn on_replayed_turn_aborted(&mut self) {
        self.submit_pending_steers_after_interrupt = false;
        self.finalize_turn();
        self.refresh_pending_input_preview();
        self.request_redraw();
    }

    /// Merge pending steers, queued drafts, and the current composer state into a single message.
    ///
    /// Each pending message numbers attachments from `[Image #1]` relative to its own remote
    /// images. When we concatenate multiple messages after interrupt, we must renumber local-image
    /// placeholders in a stable order and rebase text element byte ranges so the restored composer
    /// state stays aligned with the merged attachment list. Returns `None` when there is nothing to
    /// restore.
    pub(super) fn drain_pending_messages_for_restore(&mut self) -> Option<UserMessage> {
        if self.pending_steers.is_empty() && !self.has_queued_follow_up_messages() {
            return None;
        }

        let existing_message = UserMessage {
            text: self.bottom_pane.composer_text(),
            text_elements: self.bottom_pane.composer_text_elements(),
            local_images: self.bottom_pane.composer_local_images(),
            remote_image_urls: self.bottom_pane.remote_image_urls(),
            mention_bindings: self.bottom_pane.composer_mention_bindings(),
        };

        let mut to_merge: Vec<UserMessage> = self.rejected_steers_queue.drain(..).collect();
        to_merge.extend(
            self.pending_steers
                .drain(..)
                .map(|steer| steer.user_message),
        );
        to_merge.extend(self.queued_user_messages.drain(..));
        if !existing_message.text.is_empty()
            || !existing_message.local_images.is_empty()
            || !existing_message.remote_image_urls.is_empty()
        {
            to_merge.push(existing_message);
        }

        Some(merge_user_messages(to_merge))
    }

    pub(super) fn restore_user_message_to_composer(&mut self, user_message: UserMessage) {
        let UserMessage {
            text,
            local_images,
            remote_image_urls,
            text_elements,
            mention_bindings,
        } = user_message;
        let local_image_paths = local_images.into_iter().map(|img| img.path).collect();
        self.set_remote_image_urls(remote_image_urls);
        self.bottom_pane.set_composer_text_with_mention_bindings(
            text,
            text_elements,
            local_image_paths,
            mention_bindings,
        );
    }

    pub(crate) fn capture_thread_input_state(&self) -> Option<ThreadInputState> {
        let composer = ThreadComposerState {
            text: self.bottom_pane.composer_text(),
            text_elements: self.bottom_pane.composer_text_elements(),
            local_images: self.bottom_pane.composer_local_images(),
            remote_image_urls: self.bottom_pane.remote_image_urls(),
            mention_bindings: self.bottom_pane.composer_mention_bindings(),
            pending_pastes: self.bottom_pane.composer_pending_pastes(),
        };
        Some(ThreadInputState {
            composer: composer.has_content().then_some(composer),
            pending_steers: self
                .pending_steers
                .iter()
                .map(|pending| pending.user_message.clone())
                .collect(),
            rejected_steers_queue: self.rejected_steers_queue.clone(),
            queued_user_messages: self.queued_user_messages.clone(),
            current_collaboration_mode: self.current_collaboration_mode.clone(),
            active_collaboration_mask: self.active_collaboration_mask.clone(),
            selfwork_plan_path: self.selfwork_plan_path.clone(),
            selfwork_last_plan_digest: self.selfwork_last_plan_digest,
            selfwork_stall_count: self.selfwork_stall_count,
            selfwork_turn_in_flight: self.selfwork_turn_in_flight,
            task_running: self.bottom_pane.is_task_running(),
            agent_turn_running: self.agent_turn_running,
        })
    }

    pub(crate) fn restore_thread_input_state(&mut self, input_state: Option<ThreadInputState>) {
        let restored_task_running = input_state.as_ref().is_some_and(|state| state.task_running);
        if let Some(input_state) = input_state {
            self.current_collaboration_mode = input_state.current_collaboration_mode;
            self.active_collaboration_mask = input_state.active_collaboration_mask;
            self.selfwork_plan_path = input_state.selfwork_plan_path;
            self.selfwork_last_plan_digest = input_state.selfwork_last_plan_digest;
            self.selfwork_stall_count = input_state.selfwork_stall_count;
            self.selfwork_turn_in_flight = input_state.selfwork_turn_in_flight;
            self.sync_work_panel_selfwork();
            self.agent_turn_running = input_state.agent_turn_running;
            self.update_collaboration_mode_indicator();
            self.refresh_model_dependent_surfaces();
            if let Some(composer) = input_state.composer {
                let local_image_paths = composer
                    .local_images
                    .into_iter()
                    .map(|img| img.path)
                    .collect();
                self.set_remote_image_urls(composer.remote_image_urls);
                self.bottom_pane.set_composer_text_with_mention_bindings(
                    composer.text,
                    composer.text_elements,
                    local_image_paths,
                    composer.mention_bindings,
                );
                self.bottom_pane
                    .set_composer_pending_pastes(composer.pending_pastes);
            } else {
                self.set_remote_image_urls(Vec::new());
                self.bottom_pane.set_composer_text_with_mention_bindings(
                    String::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
                self.bottom_pane.set_composer_pending_pastes(Vec::new());
            }
            self.pending_steers = input_state
                .pending_steers
                .into_iter()
                .map(|user_message| PendingSteer {
                    compare_key: PendingSteerCompareKey {
                        message: user_message.text.clone(),
                        image_count: user_message.local_images.len()
                            + user_message.remote_image_urls.len(),
                    },
                    user_message,
                })
                .collect();
            self.rejected_steers_queue = input_state.rejected_steers_queue;
            self.queued_user_messages = input_state.queued_user_messages;
        } else {
            self.agent_turn_running = false;
            self.pending_steers.clear();
            self.rejected_steers_queue.clear();
            self.selfwork_plan_path = None;
            self.selfwork_last_plan_digest = None;
            self.selfwork_stall_count = 0;
            self.selfwork_turn_in_flight = false;
            self.sync_work_panel_selfwork();
            self.set_remote_image_urls(Vec::new());
            self.bottom_pane.set_composer_text_with_mention_bindings(
                String::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            self.bottom_pane.set_composer_pending_pastes(Vec::new());
            self.queued_user_messages.clear();
        }
        self.turn_sleep_inhibitor
            .set_turn_running(self.agent_turn_running);
        self.update_task_running_state();
        if restored_task_running && !self.bottom_pane.is_task_running() {
            self.bottom_pane.set_task_running(/*running*/ true);
            self.refresh_terminal_title();
        }
        self.refresh_pending_input_preview();
        self.request_redraw();
    }

    pub(crate) fn set_queue_autosend_suppressed(&mut self, suppressed: bool) {
        self.suppress_queue_autosend = suppressed;
    }

    // If idle and there are queued inputs, submit exactly one to start the next turn.
    pub(crate) fn maybe_send_next_queued_input(&mut self) {
        if self.suppress_queue_autosend {
            return;
        }
        if self.bottom_pane.is_task_running() {
            return;
        }
        if self.read_only_thread_control_label().is_some() {
            return;
        }
        if let Some(user_message) = self.pop_next_queued_user_message() {
            self.submit_user_message(user_message);
        }
        // Update the list to reflect the remaining queued messages (if any).
        self.refresh_pending_input_preview();
    }

    /// Rebuild and update the bottom-pane pending-input preview.
    pub(super) fn refresh_pending_input_preview(&mut self) {
        let queued_messages: Vec<String> = self
            .queued_user_messages
            .iter()
            .map(|m| m.text.clone())
            .collect();
        let pending_steers: Vec<String> = self
            .pending_steers
            .iter()
            .map(|steer| steer.user_message.text.clone())
            .collect();
        let rejected_steers: Vec<String> = self
            .rejected_steers_queue
            .iter()
            .map(|message| message.text.clone())
            .collect();
        self.bottom_pane.set_pending_input_preview(
            queued_messages,
            pending_steers,
            rejected_steers,
        );
        self.sync_work_panel_queue();
    }
}
