use super::*;

/// Parsed status-surface configuration shared by one refresh pass.
#[derive(Debug)]
pub(super) struct StatusSurfaceSelections {
    pub(super) status_line_items: Vec<StatusLineItem>,
    pub(super) invalid_status_line_items: Vec<String>,
    pub(super) terminal_title_items: Vec<TerminalTitleItem>,
    pub(super) invalid_terminal_title_items: Vec<String>,
}

/// Parsed selections retained by one materialized chat view.
///
/// The source lists are part of the key, so direct runtime config replacement invalidates the
/// cache without another lifecycle hook. Hot view restores and title animation ticks otherwise
/// reuse the parsed enums without cloning and reparsing both string lists.
#[derive(Debug)]
pub(super) struct CachedStatusSurfaceSelections {
    status_line_config: Option<Vec<String>>,
    terminal_title_config: Option<Vec<String>>,
    selections: Arc<StatusSurfaceSelections>,
}

impl ChatWidget {
    pub(super) fn status_surface_selections(&mut self) -> Arc<StatusSurfaceSelections> {
        if let Some(cached) = self.status_surface_selections_cache.as_ref()
            && cached.status_line_config.as_ref() == self.tui_config.status_line.as_ref()
            && cached.terminal_title_config.as_ref() == self.tui_config.terminal_title.as_ref()
        {
            return Arc::clone(&cached.selections);
        }

        let (status_line_items, invalid_status_line_items) = self.status_line_items_with_invalids();
        let (terminal_title_items, invalid_terminal_title_items) =
            self.terminal_title_items_with_invalids();
        let selections = Arc::new(StatusSurfaceSelections {
            status_line_items,
            invalid_status_line_items,
            terminal_title_items,
            invalid_terminal_title_items,
        });
        self.status_surface_selections_cache = Some(CachedStatusSurfaceSelections {
            status_line_config: self.tui_config.status_line.clone(),
            terminal_title_config: self.tui_config.terminal_title.clone(),
            selections: Arc::clone(&selections),
        });
        selections
    }
}
