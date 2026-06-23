use super::*;

impl App {
    pub(super) async fn handle_status_line_setup(
        &mut self,
        items: Vec<crate::bottom_pane::StatusLineItem>,
    ) {
        let ids = items.iter().map(ToString::to_string).collect::<Vec<_>>();
        let edit = tui_config::status_line_items_edit(&ids);
        let apply_result = ConfigEditsBuilder::new(&self.config.praxis_home)
            .with_edits([edit])
            .apply()
            .await;
        match apply_result {
            Ok(()) => {
                self.tui_config.status_line = Some(ids.clone());
                self.chat_widget.set_tui_config(self.tui_config.clone());
                self.chat_widget.setup_status_line(items);
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to persist status line items; keeping previous selection");
                self.chat_widget
                    .add_error_message(format!("Failed to save status line items: {err}"));
            }
        }
    }

    pub(super) fn handle_status_line_branch_updated(
        &mut self,
        cwd: PathBuf,
        branch: Option<String>,
    ) {
        self.chat_widget.set_status_line_branch(cwd, branch);
        self.refresh_status_line();
    }

    pub(super) fn handle_status_line_setup_cancelled(&mut self) {
        self.chat_widget.cancel_status_line_setup();
    }

    pub(super) async fn handle_terminal_title_setup(
        &mut self,
        items: Vec<crate::bottom_pane::TerminalTitleItem>,
    ) {
        let ids = items.iter().map(ToString::to_string).collect::<Vec<_>>();
        let edit = tui_config::terminal_title_items_edit(&ids);
        let apply_result = ConfigEditsBuilder::new(&self.config.praxis_home)
            .with_edits([edit])
            .apply()
            .await;
        match apply_result {
            Ok(()) => {
                self.tui_config.terminal_title = Some(ids.clone());
                self.chat_widget.set_tui_config(self.tui_config.clone());
                self.chat_widget.setup_terminal_title(items);
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to persist terminal title items; keeping previous selection");
                self.chat_widget.revert_terminal_title_setup_preview();
                self.chat_widget
                    .add_error_message(format!("Failed to save terminal title items: {err}"));
            }
        }
    }

    pub(super) fn handle_terminal_title_setup_preview(
        &mut self,
        items: Vec<crate::bottom_pane::TerminalTitleItem>,
    ) {
        self.chat_widget.preview_terminal_title(items);
    }

    pub(super) fn handle_terminal_title_setup_cancelled(&mut self) {
        self.chat_widget.cancel_terminal_title_setup();
    }

    pub(super) async fn handle_syntax_theme_selected(&mut self, name: String) {
        let edit = tui_config::syntax_theme_edit(&name);
        let apply_result = ConfigEditsBuilder::new(&self.config.praxis_home)
            .with_edits([edit])
            .apply()
            .await;
        match apply_result {
            Ok(()) => {
                if let Some(theme) = crate::render::highlight::resolve_theme_by_name(
                    &name,
                    Some(&self.config.praxis_home),
                ) {
                    crate::render::highlight::set_syntax_theme(theme);
                }
                self.sync_tui_theme_selection(name);
            }
            Err(err) => {
                self.restore_runtime_theme_from_config();
                tracing::error!(error = %err, "failed to persist theme selection");
                self.chat_widget
                    .add_error_message(format!("Failed to save theme: {err}"));
            }
        }
    }

    pub(super) fn handle_surface_theme_preview(
        &mut self,
        tui: &mut tui::Tui,
        name: Option<String>,
    ) {
        self.sync_tui_surface_theme_selection(name);
        tui.frame_requester().schedule_frame();
    }

    pub(super) async fn handle_surface_theme_selected(
        &mut self,
        tui: &mut tui::Tui,
        name: String,
        previous_name: Option<String>,
    ) {
        let edit = tui_config::surface_theme_edit(&name);
        let apply_result = ConfigEditsBuilder::new(&self.config.praxis_home)
            .with_edits([edit])
            .apply()
            .await;
        match apply_result {
            Ok(()) => {
                self.sync_tui_surface_theme_selection(Some(name));
                tui.frame_requester().schedule_frame();
            }
            Err(err) => {
                self.sync_tui_surface_theme_selection(previous_name);
                tracing::error!(error = %err, "failed to persist surface theme selection");
                self.chat_widget
                    .add_error_message(format!("Failed to save surface theme: {err}"));
            }
        }
    }
}
