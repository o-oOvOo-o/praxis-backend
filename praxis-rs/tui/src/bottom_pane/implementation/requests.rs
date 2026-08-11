impl BottomPane {
    pub(crate) fn composer_is_empty(&self) -> bool {
        self.composer.is_empty()
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.is_task_running
    }

    /// Return true when the pane is in the regular composer state without any
    /// overlays or popups and not running a task. This is the safe context to
    /// use Esc-Esc for backtracking from the main view.
    pub(crate) fn is_normal_backtrack_mode(&self) -> bool {
        !self.is_task_running && self.view_stack.is_empty() && !self.composer.popup_active()
    }

    /// Return true when no popups or modal views are active, regardless of task state.
    pub(crate) fn can_launch_external_editor(&self) -> bool {
        self.view_stack.is_empty() && !self.composer.popup_active()
    }

    /// Returns true when the bottom pane has no active modal view and no active composer popup.
    ///
    /// This is the UI-level definition of "no modal/popup is active" for key routing decisions.
    /// It intentionally does not include task state, since some actions are safe while a task is
    /// running and some are not.
    pub(crate) fn no_modal_or_popup_active(&self) -> bool {
        self.can_launch_external_editor()
    }

    pub(crate) fn show_view(&mut self, view: Box<dyn BottomPaneView>) {
        self.push_view(view);
    }

    /// Called when the agent requests user approval.
    pub fn push_approval_request(&mut self, request: ApprovalRequest, features: &Features) {
        let request = if let Some(view) = self.view_stack.last_mut() {
            match view.try_consume_approval_request(request) {
                Some(request) => request,
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            request
        };

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalOverlay::new(request, self.app_event_tx.clone(), features.clone());
        self.pause_status_timer_for_modal();
        self.push_view(Box::new(modal));
    }

    pub(crate) fn auto_approve_runtime_approval_requests(&mut self) -> bool {
        let Some(view) = self.view_stack.last_mut() else {
            return false;
        };
        let changed = view.auto_approve_runtime_approval_requests();
        if view.is_complete() {
            self.view_stack.pop();
            self.on_active_view_complete();
        }
        if changed {
            self.request_redraw();
        }
        changed
    }

    /// Called when the agent requests user input.
    pub fn push_user_input_request(&mut self, request: RequestUserInputEvent) {
        let request = if let Some(view) = self.view_stack.last_mut() {
            match view.try_consume_user_input_request(request) {
                Some(request) => request,
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            request
        };

        let modal = RequestUserInputOverlay::new(
            request,
            self.app_event_tx.clone(),
            self.has_input_focus,
            self.enhanced_keys_supported,
            self.disable_paste_burst,
        );
        self.pause_status_timer_for_modal();
        self.set_composer_input_enabled(
            /*enabled*/ false,
            Some("Answer the questions to continue.".to_string()),
        );
        self.push_view(Box::new(modal));
    }

    pub(crate) fn push_mcp_server_elicitation_request(
        &mut self,
        request: McpServerElicitationFormRequest,
    ) {
        let request = if let Some(view) = self.view_stack.last_mut() {
            match view.try_consume_mcp_server_elicitation_request(request) {
                Some(request) => request,
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            request
        };

        if let Some(tool_suggestion) = request.tool_suggestion()
            && let Some(install_url) = tool_suggestion.install_url.clone()
        {
            let suggestion_type = match tool_suggestion.suggest_type {
                mcp_server_elicitation::ToolSuggestionType::Install => {
                    AppLinkSuggestionType::Install
                }
                mcp_server_elicitation::ToolSuggestionType::Enable => AppLinkSuggestionType::Enable,
            };
            let is_installed = matches!(
                tool_suggestion.suggest_type,
                mcp_server_elicitation::ToolSuggestionType::Enable
            );
            let view = AppLinkView::new(
                AppLinkViewParams {
                    app_id: tool_suggestion.tool_id.clone(),
                    title: tool_suggestion.tool_name.clone(),
                    description: None,
                    instructions: match suggestion_type {
                        AppLinkSuggestionType::Install => {
                            "Install this app in your browser, then return here.".to_string()
                        }
                        AppLinkSuggestionType::Enable => {
                            "Enable this app to use it for the current request.".to_string()
                        }
                    },
                    url: install_url,
                    is_installed,
                    is_enabled: false,
                    suggest_reason: Some(tool_suggestion.suggest_reason.clone()),
                    suggestion_type: Some(suggestion_type),
                    elicitation_target: Some(AppLinkElicitationTarget {
                        thread_id: request.thread_id(),
                        server_name: request.server_name().to_string(),
                        request_id: request.request_id().clone(),
                    }),
                },
                self.app_event_tx.clone(),
            );
            self.pause_status_timer_for_modal();
            self.set_composer_input_enabled(
                /*enabled*/ false,
                Some("Respond to the tool suggestion to continue.".to_string()),
            );
            self.push_view(Box::new(view));
            return;
        }

        let modal = McpServerElicitationOverlay::new(
            request,
            self.app_event_tx.clone(),
            self.has_input_focus,
            self.enhanced_keys_supported,
            self.disable_paste_burst,
        );
        self.pause_status_timer_for_modal();
        self.set_composer_input_enabled(
            /*enabled*/ false,
            Some("Respond to the MCP server request to continue.".to_string()),
        );
        self.push_view(Box::new(modal));
    }

    fn on_active_view_complete(&mut self) {
        self.resume_status_timer_after_modal();
        self.set_composer_input_enabled(/*enabled*/ true, /*placeholder*/ None);
    }

    fn pause_status_timer_for_modal(&mut self) {
        if let Some(status) = self.status.as_mut() {
            status.pause_timer();
        }
    }

    fn resume_status_timer_after_modal(&mut self) {
        if let Some(status) = self.status.as_mut() {
            status.resume_timer();
        }
    }
}
