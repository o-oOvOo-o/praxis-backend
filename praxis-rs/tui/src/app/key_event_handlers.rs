use super::*;

impl App {
    pub(super) async fn handle_key_event(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        key_event: KeyEvent,
    ) {
        if self.handle_mouse_selection_copy_shortcut(key_event) {
            tui.frame_requester().schedule_frame();
            return;
        }
        if key_event.kind == KeyEventKind::Press {
            self.mouse.selection = None;
        }

        // Some terminals, especially on macOS, encode Option+Left/Right as Option+b/f unless
        // enhanced keyboard reporting is available. We only treat those word-motion fallbacks as
        // agent-switch shortcuts when the composer is empty so we never steal the expected
        // editing behavior for moving across words inside a draft.
        let allow_agent_word_motion_fallback = !self.enhanced_keys_supported
            && self.chat_widget.composer_text_with_pending().is_empty();
        if self.overlay.is_none()
            && self.chat_widget.no_modal_or_popup_active()
            // Alt+Left/Right are also natural word-motion keys in the composer. Keep agent
            // fast-switch available only once the draft is empty so editing behavior wins whenever
            // there is text on screen.
            && self.chat_widget.composer_text_with_pending().is_empty()
            && previous_agent_shortcut_matches(key_event, allow_agent_word_motion_fallback)
        {
            if let Some(thread_id) = self
                .adjacent_thread_id_with_backfill(app_gateway, AgentNavigationDirection::Previous)
                .await
            {
                let _ = self.select_agent_thread(tui, app_gateway, thread_id).await;
            }
            return;
        }
        if self.overlay.is_none()
            && self.chat_widget.no_modal_or_popup_active()
            // Mirror the previous-agent rule above: empty drafts may use these keys for thread
            // switching, but non-empty drafts keep them for expected word-wise cursor motion.
            && self.chat_widget.composer_text_with_pending().is_empty()
            && next_agent_shortcut_matches(key_event, allow_agent_word_motion_fallback)
        {
            if let Some(thread_id) = self
                .adjacent_thread_id_with_backfill(app_gateway, AgentNavigationDirection::Next)
                .await
            {
                let _ = self.select_agent_thread(tui, app_gateway, thread_id).await;
            }
            return;
        }

        if self
            .handle_workspace_key_event(tui, app_gateway, key_event)
            .await
        {
            return;
        }

        match key_event {
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                self.open_transcript_overlay(tui);
            }
            KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                if !self.chat_widget.can_run_ctrl_l_clear_now() {
                    return;
                }
                if let Err(err) = self.clear_terminal_ui(tui, /*redraw_header*/ false) {
                    tracing::warn!(error = %err, "failed to clear terminal UI");
                    self.chat_widget
                        .add_error_message(format!("Failed to clear terminal UI: {err}"));
                } else {
                    self.reset_app_ui_state_after_clear();
                    self.queue_clear_ui_header(tui);
                    tui.frame_requester().schedule_frame();
                }
            }
            KeyEvent {
                code: KeyCode::Char('g'),
                modifiers: crossterm::event::KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                ..
            } => {
                // Only launch the external editor if there is no overlay and the bottom pane is not in use.
                // Note that it can be launched while a task is running to enable editing while the previous turn is ongoing.
                if self.overlay.is_none()
                    && self.chat_widget.can_launch_external_editor()
                    && self.chat_widget.external_editor_state() == ExternalEditorState::Closed
                {
                    self.request_external_editor_launch(tui);
                }
            }
            // Esc primes/advances backtracking only in normal (not working) mode
            // with the composer focused and empty. In any other state, forward
            // Esc so the active UI (e.g. status indicator, modals, popups)
            // handles it.
            KeyEvent {
                code: KeyCode::Esc,
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                if self.chat_widget.is_normal_backtrack_mode()
                    && self.chat_widget.composer_is_empty()
                {
                    self.handle_backtrack_esc_key(tui);
                } else {
                    self.chat_widget.handle_key_event(key_event);
                }
            }
            // Enter confirms backtrack when primed + count > 0. Otherwise pass to widget.
            KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                ..
            } if self.backtrack.primed
                && self.backtrack.nth_user_message != usize::MAX
                && self.chat_widget.composer_is_empty() =>
            {
                if let Some(selection) = self.confirm_backtrack_from_main() {
                    self.apply_backtrack_selection(tui, selection);
                }
            }
            KeyEvent {
                kind: KeyEventKind::Press | KeyEventKind::Repeat,
                ..
            } => {
                // Any non-Esc key press should cancel a primed backtrack.
                // This avoids stale "Esc-primed" state after the user starts typing
                // (even if they later backspace to empty).
                if key_event.code != KeyCode::Esc && self.backtrack.primed {
                    self.reset_backtrack_state();
                }
                self.chat_widget.handle_key_event(key_event);
            }
            _ => {
                self.chat_widget.handle_key_event(key_event);
            }
        };
    }

    pub(super) async fn handle_workspace_key_event(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        key_event: KeyEvent,
    ) -> bool {
        if !self.workspace.enabled
            || self.overlay.is_some()
            || !self.chat_widget.no_modal_or_popup_active()
            || !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            return false;
        }

        if !matches!(self.workspace.overlay, WorkspaceOverlay::None) {
            return self
                .handle_workspace_overlay_key(tui, app_gateway, key_event)
                .await;
        }

        if let Some(effect) = self.workspace.handle_main_pane_key(key_event) {
            let error_context = effect.error_context();
            if let Err(err) = self
                .handle_workspace_main_pane_effect(tui, app_gateway, effect)
                .await
            {
                self.chat_widget
                    .add_error_message(format!("{error_context} failed: {err}"));
            }
            tui.frame_requester().schedule_frame();
            return true;
        }

        if self.workspace.search_focused {
            let handled = self.handle_workspace_search_key(app_gateway, key_event);
            tui.frame_requester().schedule_frame();
            return handled;
        }

        if key_event.code == KeyCode::Char('o')
            && key_event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            && self.chat_widget.composer_text_with_pending().is_empty()
        {
            self.workspace.clear_search_focus();
            let _ = self
                .execute_workspace_chrome_action(
                    tui,
                    app_gateway,
                    WorkspaceChromeAction::OpenFolder,
                )
                .await;
            tui.frame_requester().schedule_frame();
            return true;
        }

        if key_event.code == KeyCode::Char('n')
            && key_event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            && self.chat_widget.composer_text_with_pending().is_empty()
        {
            self.workspace.clear_overlay();
            self.workspace.clear_search_focus();
            self.start_fresh_session_with_summary_hint(tui, app_gateway)
                .await;
            self.refresh_workspace_threads(app_gateway, true);
            tui.frame_requester().schedule_frame();
            return true;
        }

        if !self.chat_widget.composer_text_with_pending().is_empty() {
            return false;
        }

        let visible_rows = self.workspace_visible_row_capacity();
        let item_count = self.workspace.list_item_count();
        match key_event.code {
            KeyCode::Up => self.workspace.select_previous(),
            KeyCode::Down => self.workspace.select_next(item_count),
            KeyCode::PageUp => self.workspace.page_selection_up(visible_rows),
            KeyCode::PageDown => self.workspace.page_selection_down(visible_rows, item_count),
            KeyCode::Home => self.workspace.select_first(),
            KeyCode::End => self.workspace.select_last(item_count),
            KeyCode::Enter => {
                if self.workspace.is_load_more_index(self.workspace.selected) {
                    self.load_more_workspace_threads(app_gateway);
                    tui.frame_requester().schedule_frame();
                    return true;
                }
                if self
                    .workspace
                    .toggle_selected_closed_subagents(self.workspace_visible_row_capacity())
                {
                    tui.frame_requester().schedule_frame();
                    return true;
                }
                let Some(row_index) = self
                    .workspace
                    .actual_row_index_for_visible(self.workspace.selected)
                else {
                    return true;
                };
                let Some(row) = self.workspace.rows.get(row_index).cloned() else {
                    return true;
                };
                let _ = self
                    .resume_session_target(
                        tui,
                        app_gateway,
                        SessionTarget {
                            path: row.path,
                            thread_id: row.thread_id,
                            thread_name: Some(row.name),
                            cwd: Some(row.cwd),
                        },
                    )
                    .await;
                return true;
            }
            KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete => {
                tui.frame_requester().schedule_frame();
                return false;
            }
            _ => return false,
        }

        self.workspace.ensure_selected_visible(visible_rows);
        tui.frame_requester().schedule_frame();
        true
    }

    pub(super) async fn handle_workspace_main_pane_effect(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        effect: WorkspaceMainPaneEffect,
    ) -> Result<Option<AppRunControl>> {
        let Some(effect) = effect.into_gateway_effect() else {
            return Ok(None);
        };
        let schedules_frame = effect.schedules_frame_after_apply();
        let result = self
            .handle_workspace_gateway_effect(tui, app_gateway, effect)
            .await;
        if schedules_frame {
            tui.frame_requester().schedule_frame();
        }
        result
    }

    pub(super) async fn handle_workspace_gateway_effect(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        effect: WorkspaceGatewayEffect,
    ) -> Result<Option<AppRunControl>> {
        match effect {
            WorkspaceGatewayEffect::LoadSessionPickerPage(request) => {
                self.queue_workspace_session_picker_page(request).await;
                Ok(None)
            }
            WorkspaceGatewayEffect::SelectSession(selection) => {
                self.apply_session_selection(tui, app_gateway, selection)
                    .await
            }
            WorkspaceGatewayEffect::SelectAgent(thread_id) => {
                self.select_agent_thread(tui, app_gateway, thread_id)
                    .await?;
                Ok(None)
            }
        }
    }

    pub(super) fn handle_workspace_search_key(
        &mut self,
        app_gateway: &AppGatewaySession,
        key_event: KeyEvent,
    ) -> bool {
        let mut changed = false;
        match key_event.code {
            KeyCode::Esc => {
                if self.workspace.search_query.is_empty() {
                    self.workspace.search_focused = false;
                } else {
                    self.workspace.search_query.clear();
                    changed = true;
                }
            }
            KeyCode::Enter => self.workspace.search_focused = false,
            KeyCode::Backspace => {
                changed = self.workspace.search_query.pop().is_some();
            }
            KeyCode::Delete => {
                changed = !self.workspace.search_query.is_empty();
                self.workspace.search_query.clear();
            }
            KeyCode::Char('u')
                if key_event
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                changed = !self.workspace.search_query.is_empty();
                self.workspace.search_query.clear();
            }
            KeyCode::Char(c)
                if key_event.modifiers.is_empty()
                    || key_event.modifiers == crossterm::event::KeyModifiers::SHIFT =>
            {
                self.workspace.search_query.push(c);
                changed = true;
            }
            _ => {}
        }

        if changed {
            self.workspace.reset_selection_and_list_scroll();
            self.workspace.refresh_in_flight = false;
            self.workspace.last_refresh_at = None;
            self.refresh_workspace_threads(app_gateway, true);
        }
        true
    }

    pub(super) async fn handle_workspace_overlay_key(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        key_event: KeyEvent,
    ) -> bool {
        match self.workspace.overlay.clone() {
            WorkspaceOverlay::ChromeMenu(menu) => match key_event.code {
                KeyCode::Esc => self.workspace.clear_overlay(),
                KeyCode::Left | KeyCode::Right => {
                    let next_menu = match menu.menu {
                        WorkspaceChromeMenu::File => WorkspaceChromeMenu::Help,
                        WorkspaceChromeMenu::Help => WorkspaceChromeMenu::File,
                    };
                    self.workspace.overlay =
                        WorkspaceOverlay::ChromeMenu(WorkspaceChromeMenuState {
                            menu: next_menu,
                            selected: 0,
                            area: None,
                        });
                }
                KeyCode::Up => {
                    if let WorkspaceOverlay::ChromeMenu(current) = &mut self.workspace.overlay {
                        current.selected = current.selected.saturating_sub(1);
                    }
                }
                KeyCode::Down => {
                    if let WorkspaceOverlay::ChromeMenu(current) = &mut self.workspace.overlay {
                        current.selected = current.selected.saturating_add(1).min(
                            workspace_chrome_menu_actions(current.menu)
                                .len()
                                .saturating_sub(1),
                        );
                    }
                }
                KeyCode::Enter => {
                    let action = workspace_chrome_menu_actions(menu.menu)
                        .get(menu.selected)
                        .copied();
                    if let Some(action) = action {
                        let _ = self
                            .execute_workspace_chrome_action(tui, app_gateway, action)
                            .await;
                    }
                }
                _ => {}
            },
            WorkspaceOverlay::OpenFolder(_) => match key_event.code {
                KeyCode::Esc => self.workspace.clear_overlay(),
                KeyCode::Enter => {
                    let _ = self.commit_workspace_open_folder(tui, app_gateway).await;
                }
                KeyCode::Home => {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
                        prompt.cursor = 0;
                        prompt.message = None;
                    }
                }
                KeyCode::End => {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
                        prompt.cursor = prompt.value.len();
                        prompt.message = None;
                    }
                }
                KeyCode::Left => {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
                        prompt.cursor = previous_char_boundary(&prompt.value, prompt.cursor);
                        prompt.message = None;
                    }
                }
                KeyCode::Right => {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
                        prompt.cursor = next_char_boundary(&prompt.value, prompt.cursor);
                        prompt.message = None;
                    }
                }
                KeyCode::Backspace => {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay
                        && prompt.cursor > 0
                    {
                        let previous = previous_char_boundary(&prompt.value, prompt.cursor);
                        prompt.value.drain(previous..prompt.cursor);
                        prompt.cursor = previous;
                        prompt.message = None;
                    }
                }
                KeyCode::Delete => {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay
                        && prompt.cursor < prompt.value.len()
                    {
                        let next = next_char_boundary(&prompt.value, prompt.cursor);
                        prompt.value.drain(prompt.cursor..next);
                        prompt.message = None;
                    }
                }
                KeyCode::Char('u')
                    if key_event
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
                        prompt.value.clear();
                        prompt.cursor = 0;
                        prompt.message = None;
                    }
                }
                KeyCode::Char(c)
                    if key_event.modifiers.is_empty()
                        || key_event.modifiers == crossterm::event::KeyModifiers::SHIFT =>
                {
                    if let WorkspaceOverlay::OpenFolder(prompt) = &mut self.workspace.overlay {
                        prompt.value.insert(prompt.cursor, c);
                        prompt.cursor += c.len_utf8();
                        prompt.message = None;
                    }
                }
                _ => {}
            },
            WorkspaceOverlay::ContextMenu(menu) => match key_event.code {
                KeyCode::Esc => {
                    self.workspace.clear_overlay();
                }
                KeyCode::Up => {
                    if let WorkspaceOverlay::ContextMenu(current) = &mut self.workspace.overlay {
                        current.selected = current.selected.saturating_sub(1);
                    }
                }
                KeyCode::Down => {
                    if let WorkspaceOverlay::ContextMenu(current) = &mut self.workspace.overlay {
                        current.selected = current
                            .selected
                            .saturating_add(1)
                            .min(workspace_menu_actions().len().saturating_sub(1));
                    }
                }
                KeyCode::Enter => {
                    let action = workspace_menu_actions()
                        .get(menu.selected)
                        .copied()
                        .unwrap_or(WorkspaceMenuAction::Open);
                    let _ = self
                        .execute_workspace_menu_action(tui, app_gateway, action)
                        .await;
                }
                _ => {}
            },
            WorkspaceOverlay::Rename(_) => match key_event.code {
                KeyCode::Esc => self.workspace.clear_overlay(),
                KeyCode::Enter => self.commit_workspace_rename(app_gateway).await,
                KeyCode::Home => {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay {
                        rename.cursor = 0;
                    }
                }
                KeyCode::End => {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay {
                        rename.cursor = rename.value.len();
                    }
                }
                KeyCode::Left => {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay {
                        rename.cursor = previous_char_boundary(&rename.value, rename.cursor);
                    }
                }
                KeyCode::Right => {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay {
                        rename.cursor = next_char_boundary(&rename.value, rename.cursor);
                    }
                }
                KeyCode::Backspace => {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay
                        && rename.cursor > 0
                    {
                        let previous = previous_char_boundary(&rename.value, rename.cursor);
                        rename.value.drain(previous..rename.cursor);
                        rename.cursor = previous;
                    }
                }
                KeyCode::Delete => {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay
                        && rename.cursor < rename.value.len()
                    {
                        let next = next_char_boundary(&rename.value, rename.cursor);
                        rename.value.drain(rename.cursor..next);
                    }
                }
                KeyCode::Char('u')
                    if key_event
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay {
                        rename.value.clear();
                        rename.cursor = 0;
                    }
                }
                KeyCode::Char(c)
                    if key_event.modifiers.is_empty()
                        || key_event.modifiers == crossterm::event::KeyModifiers::SHIFT =>
                {
                    if let WorkspaceOverlay::Rename(rename) = &mut self.workspace.overlay {
                        rename.value.insert(rename.cursor, c);
                        rename.cursor += c.len_utf8();
                    }
                }
                _ => {}
            },
            WorkspaceOverlay::ConfirmArchive(_) => match key_event.code {
                KeyCode::Esc | KeyCode::Char('n') => self.workspace.clear_overlay(),
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.confirm_workspace_archive(tui, app_gateway).await;
                }
                _ => {}
            },
            WorkspaceOverlay::ConfirmDelete(_) => match key_event.code {
                KeyCode::Esc | KeyCode::Char('n') => self.workspace.clear_overlay(),
                KeyCode::Enter | KeyCode::Char('y') => {
                    self.confirm_workspace_delete(tui, app_gateway).await;
                }
                _ => {}
            },
            WorkspaceOverlay::None => {}
        }
        tui.frame_requester().schedule_frame();
        true
    }
}
