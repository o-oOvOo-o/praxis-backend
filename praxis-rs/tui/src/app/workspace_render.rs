use super::App;
use super::mouse_interaction::MousePane;
use super::mouse_interaction::WorkspaceMouseTarget;
use super::workspace_view_helpers::render_workspace_row_accent;
use super::workspace_view_helpers::workspace_cache_label;
use super::workspace_view_helpers::workspace_row_accent;
use super::workspace_view_helpers::workspace_row_style;
use super::workspace_view_helpers::workspace_status_style;
use super::workspace_view_helpers::workspace_truncate;
use crate::tui2::InputVisual;
use crate::ui_language::UiLanguage;
use crate::workspace::WORKSPACE_CHROME_HEIGHT;
use crate::workspace::WORKSPACE_SUBAGENT_INDENT_STEP;
use crate::workspace::WorkspaceChromeBarAreas;
use crate::workspace::WorkspaceChromeMenu;
use crate::workspace::WorkspaceChromeMenuState;
use crate::workspace::WorkspaceMenuAction;
use crate::workspace::WorkspaceOverlay;
use crate::workspace::WorkspacePaneSplit;
use crate::workspace::WorkspaceVisibleItem;
use crate::workspace::refresh_workspace_subagent_summaries;
use crate::workspace::workspace_chrome_action_label;
use crate::workspace::workspace_chrome_action_shortcut;
use crate::workspace::workspace_chrome_menu_actions;
use crate::workspace::workspace_chrome_menu_bar_areas;
use crate::workspace::workspace_chrome_menu_popup_area;
use crate::workspace::workspace_chrome_menu_title;
use crate::workspace::workspace_closed_subagents_detail;
use crate::workspace::workspace_closed_subagents_label;
use crate::workspace::workspace_context_subagent_lines;
use crate::workspace::workspace_control_detail;
use crate::workspace::workspace_dialog_area;
use crate::workspace::workspace_menu_action_disabled;
use crate::workspace::workspace_menu_action_label;
use crate::workspace::workspace_menu_actions;
use crate::workspace::workspace_pane_split;
use crate::workspace::workspace_popup_area;
use crate::workspace::workspace_row_control_marker;
use crate::workspace::workspace_row_is_controlled;
use crate::workspace::workspace_row_status_label;
use crate::workspace::workspace_row_subagent_marker;
use crate::workspace::workspace_row_tree_indent;
use crate::workspace::workspace_row_tree_prefix;
use crate::workspace::workspace_thread_display_name;
use crate::workspace::workspace_toolbar_areas;
use crate::workspace::workspace_window_inner_area;
use praxis_app_gateway_protocol::ThreadStatus;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

pub(super) const WORKSPACE_LIST_TOP_PADDING: u16 = 4;
pub(super) const WORKSPACE_ROW_HEIGHT: u16 = 3;
const WORKSPACE_CONTEXT_MENU_WIDTH: u16 = 36;
const WORKSPACE_RENAME_POPUP_WIDTH: u16 = 34;
const WORKSPACE_CONFIRM_POPUP_WIDTH: u16 = 30;
const WORKSPACE_STATUS_LABEL_WIDTH: usize = 10;

impl App {
    pub(super) fn workspace_visible_row_capacity(&self) -> usize {
        self.workspace
            .list_area
            .map(|area| {
                ((area.height.saturating_sub(WORKSPACE_LIST_TOP_PADDING)) / WORKSPACE_ROW_HEIGHT)
                    as usize
            })
            .unwrap_or(0)
    }

    pub(super) fn render_workspace_or_chat(
        &mut self,
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
    ) -> Rect {
        if !self.workspace.enabled {
            self.workspace.list_area = None;
            self.workspace.chat_area = Some(area);
            self.workspace.work_panel_area = None;
            self.workspace.chrome_bar_area = None;
            self.workspace.chrome_bar_areas = WorkspaceChromeBarAreas::default();
            self.workspace.toolbar_new_area = None;
            self.workspace.toolbar_search_area = None;
            self.mouse.hover_workspace_thread_index = None;
            self.mouse.hover_workspace_target = None;
            self.mouse.workspace_list_snapshot = None;
            self.chat_widget.render_standalone_chat(
                area,
                buf,
                &self.transcript_cells,
                self.workspace.chat_scroll_from_bottom(),
                &self.workspace.launch,
            );
            let selection_areas = self
                .chat_widget
                .standalone_mouse_selection_areas(area, &self.transcript_cells);
            self.workspace.work_panel_area = selection_areas.work_panel;
            if let Some(transcript_area) = selection_areas.transcript {
                self.update_mouse_pane_snapshot(MousePane::Chat, transcript_area, buf);
            } else {
                self.clear_mouse_pane_snapshot(MousePane::Chat);
            }
            if let Some(work_panel_area) = selection_areas.work_panel {
                self.update_mouse_pane_snapshot(MousePane::WorkPanel, work_panel_area, buf);
            } else {
                self.clear_mouse_pane_snapshot(MousePane::WorkPanel);
            }
            self.render_mouse_selection_overlay(buf);
            return area;
        }

        let theme = self.chat_widget.workspace_theme();
        let desktop_area = area;
        crate::surface::render_main_surface(desktop_area, buf, theme, None);
        let window_inner_area = workspace_window_inner_area(desktop_area);
        let chrome_area = Rect::new(
            window_inner_area.x,
            window_inner_area.y,
            window_inner_area.width,
            WORKSPACE_CHROME_HEIGHT.min(window_inner_area.height),
        );
        self.render_workspace_chrome_bar(chrome_area, buf);
        let content_y = window_inner_area.y.saturating_add(WORKSPACE_CHROME_HEIGHT);
        let content_area = Rect::new(
            window_inner_area.x,
            content_y,
            window_inner_area.width,
            window_inner_area
                .bottom()
                .saturating_sub(content_y)
                .saturating_sub(0),
        );
        let WorkspacePaneSplit {
            list_area,
            gap_area,
            chat_area,
        } = workspace_pane_split(content_area);
        self.workspace.list_area = Some(list_area);
        self.workspace.chat_area = (!chat_area.is_empty()).then_some(chat_area);
        self.workspace.work_panel_area = None;
        self.workspace
            .clamp_list_scroll(self.workspace_visible_row_capacity());
        let max_chat_scroll = if chat_area.is_empty() || !self.workspace.chat_pane_is_active() {
            0
        } else {
            self.chat_widget.workspace_chat_scroll_limit_embedded(
                chat_area,
                &self.transcript_cells,
                self.workspace.chat_scroll_from_bottom(),
            )
        };
        self.workspace.clamp_chat_scroll(max_chat_scroll);
        if !gap_area.is_empty() {
            buf.set_style(
                gap_area,
                Style::default()
                    .bg(theme.panel_bg)
                    .fg(theme.border_muted)
                    .add_modifier(Modifier::BOLD),
            );
            for y in gap_area.y..gap_area.bottom() {
                buf[(gap_area.x, y)].set_symbol("│");
            }
        }
        self.render_workspace_list(list_area, buf, false);
        if !chat_area.is_empty() {
            if self.workspace.render_picker_pane(chat_area, buf) {
                self.clear_mouse_pane_snapshot(MousePane::WorkPanel);
                self.update_mouse_pane_snapshot(MousePane::Chat, chat_area, buf);
            } else {
                self.chat_widget.render_workspace_chat_embedded(
                    chat_area,
                    buf,
                    &self.transcript_cells,
                    self.workspace.chat_scroll_from_bottom(),
                    &self.workspace.launch,
                );
                let selection_areas = self
                    .chat_widget
                    .embedded_mouse_selection_areas(chat_area, &self.transcript_cells);
                self.workspace.work_panel_area = selection_areas.work_panel;
                if let Some(transcript_area) = selection_areas.transcript {
                    self.update_mouse_pane_snapshot(MousePane::Chat, transcript_area, buf);
                } else {
                    self.clear_mouse_pane_snapshot(MousePane::Chat);
                }
                if let Some(work_panel_area) = selection_areas.work_panel {
                    self.update_mouse_pane_snapshot(MousePane::WorkPanel, work_panel_area, buf);
                } else {
                    self.clear_mouse_pane_snapshot(MousePane::WorkPanel);
                }
            }
        } else {
            self.clear_mouse_pane_snapshot(MousePane::Chat);
            self.clear_mouse_pane_snapshot(MousePane::WorkPanel);
        }
        self.update_mouse_pane_snapshot(MousePane::WorkspaceList, list_area, buf);
        self.render_workspace_chrome_overlay(window_inner_area, buf);
        self.render_mouse_selection_overlay(buf);
        chat_area
    }

    fn render_workspace_chrome_bar(&mut self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        if area.is_empty() {
            self.workspace.chrome_bar_area = None;
            self.workspace.chrome_bar_areas = WorkspaceChromeBarAreas::default();
            return;
        }
        let theme = self.chat_widget.workspace_theme();
        let language = self.chat_widget.ui_language();
        let areas = workspace_chrome_menu_bar_areas(area, language);
        self.workspace.chrome_bar_area = Some(area);
        self.workspace.chrome_bar_areas = areas;
        buf.set_style(
            area,
            Style::default()
                .bg(theme.header_bg)
                .fg(theme.text)
                .add_modifier(Modifier::empty()),
        );
        for (menu, menu_area) in [
            (WorkspaceChromeMenu::File, areas.file),
            (WorkspaceChromeMenu::Help, areas.help),
        ] {
            if menu_area.is_empty() {
                continue;
            }
            let active = matches!(
                self.workspace.overlay,
                WorkspaceOverlay::ChromeMenu(WorkspaceChromeMenuState { menu: active, .. })
                    if active == menu
            );
            let hovered =
                self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::ChromeMenu(menu));
            let style = if active {
                Style::default()
                    .bg(theme.selected_bg)
                    .fg(theme.text_strong)
                    .add_modifier(Modifier::BOLD)
            } else if hovered {
                Style::default().bg(theme.hover_bg).fg(theme.text_strong)
            } else {
                Style::default().bg(theme.header_bg).fg(theme.text)
            };
            let label = format!(" {} ", workspace_chrome_menu_title(menu, language));
            Paragraph::new(label).style(style).render(menu_area, buf);
        }
    }

    fn render_workspace_chrome_overlay(
        &mut self,
        shell_area: Rect,
        buf: &mut ratatui::buffer::Buffer,
    ) {
        let overlay = self.workspace.overlay.clone();
        let language = self.chat_widget.ui_language();
        let theme = self.chat_widget.workspace_theme();
        match overlay {
            WorkspaceOverlay::ChromeMenu(menu_state) => {
                let anchor = match menu_state.menu {
                    WorkspaceChromeMenu::File => self.workspace.chrome_bar_areas.file,
                    WorkspaceChromeMenu::Help => self.workspace.chrome_bar_areas.help,
                };
                let area =
                    workspace_chrome_menu_popup_area(shell_area, anchor, menu_state.menu, language);
                if let WorkspaceOverlay::ChromeMenu(current) = &mut self.workspace.overlay {
                    current.area = Some(area);
                }
                crate::surface::render_popup_surface(
                    area,
                    buf,
                    theme,
                    Some(Line::from(workspace_chrome_menu_title(
                        menu_state.menu,
                        language,
                    ))),
                );
                let actions = workspace_chrome_menu_actions(menu_state.menu);
                for (index, action) in actions.iter().copied().enumerate() {
                    let y = area.y.saturating_add(1 + index as u16);
                    if y >= area.bottom().saturating_sub(1) {
                        break;
                    }
                    let hovered = self.mouse.hover_workspace_target
                        == Some(WorkspaceMouseTarget::ChromeAction(action));
                    let selected = hovered || index == menu_state.selected;
                    let style = if selected {
                        Style::default().bg(theme.selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(theme.dropdown_bg).fg(theme.text)
                    };
                    let label = workspace_chrome_action_label(action, language);
                    let shortcut = workspace_chrome_action_shortcut(action);
                    let line_width = area.width.saturating_sub(2) as usize;
                    let text = if shortcut.is_empty() {
                        workspace_truncate(label, line_width)
                    } else {
                        let shortcut_width = shortcut.chars().count();
                        let label_width = line_width.saturating_sub(shortcut_width + 2);
                        format!(
                            "{:<label_width$}  {}",
                            workspace_truncate(label, label_width),
                            shortcut
                        )
                    };
                    Paragraph::new(text).style(style).render(
                        Rect::new(area.x.saturating_add(1), y, area.width.saturating_sub(2), 1),
                        buf,
                    );
                }
            }
            WorkspaceOverlay::OpenFolder(prompt) => {
                let area = workspace_dialog_area(shell_area, 68, 8);
                if let WorkspaceOverlay::OpenFolder(current) = &mut self.workspace.overlay {
                    current.area = Some(area);
                }
                let title = match language {
                    UiLanguage::En => " Open Folder ",
                    UiLanguage::Cn => " 打开文件夹 ",
                };
                crate::surface::render_popup_surface(area, buf, theme, Some(Line::from(title)));
                let label = match language {
                    UiLanguage::En => "Workspace path",
                    UiLanguage::Cn => "工作目录路径",
                };
                Paragraph::new(label)
                    .style(Style::default().bg(theme.dropdown_bg).fg(theme.muted))
                    .render(
                        Rect::new(area.x + 2, area.y + 1, area.width.saturating_sub(4), 1),
                        buf,
                    );
                let mut value = prompt.value.clone();
                let cursor = prompt.cursor.min(value.len());
                value.insert(cursor, '|');
                Paragraph::new(workspace_truncate(
                    &value,
                    area.width.saturating_sub(4) as usize,
                ))
                .style(Style::default().bg(theme.input_bg).fg(theme.text_strong))
                .render(
                    Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), 1),
                    buf,
                );
                let detail = match prompt.message.as_deref() {
                    Some(message) => message,
                    None => match language {
                        UiLanguage::En => {
                            "Enter an existing folder. Relative paths resolve from the current workspace."
                        }
                        UiLanguage::Cn => "输入已存在的文件夹。相对路径会基于当前工作目录解析。",
                    },
                };
                Paragraph::new(workspace_truncate(
                    detail,
                    area.width.saturating_sub(4) as usize,
                ))
                .style(Style::default().bg(theme.dropdown_bg).fg(theme.muted))
                .render(
                    Rect::new(area.x + 2, area.y + 4, area.width.saturating_sub(4), 1),
                    buf,
                );
                let confirm_hovered = self.mouse.hover_workspace_target
                    == Some(WorkspaceMouseTarget::OpenFolderConfirm);
                let cancel_hovered = self.mouse.hover_workspace_target
                    == Some(WorkspaceMouseTarget::OpenFolderCancel);
                let confirm_label = match language {
                    UiLanguage::En => " Open ",
                    UiLanguage::Cn => " 打开 ",
                };
                Paragraph::new(confirm_label)
                    .style(if confirm_hovered {
                        Style::default().bg(theme.selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(theme.dropdown_bg).fg(theme.accent)
                    })
                    .render(Rect::new(area.x + 2, area.y + 6, 10, 1), buf);
                Paragraph::new(language.workspace_cancel_label())
                    .style(if cancel_hovered {
                        Style::default().bg(theme.selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(theme.dropdown_bg).fg(theme.text)
                    })
                    .render(Rect::new(area.x + 14, area.y + 6, 12, 1), buf);
            }
            _ => {}
        }
    }

    fn render_workspace_list(
        &mut self,
        area: Rect,
        buf: &mut ratatui::buffer::Buffer,
        framed: bool,
    ) {
        refresh_workspace_subagent_summaries(&mut self.workspace.rows);
        let theme = self.chat_widget.workspace_theme();
        let text = theme.text;
        let muted = theme.muted;
        let green = theme.accent;
        let controlled_accent = theme.control_accent;
        let controlled_muted = theme.control_muted;
        let ui_palette = theme.ui_palette();
        let language = self.chat_widget.ui_language();
        buf.set_style(area, Style::default().bg(theme.panel_bg).fg(text));
        let title = Line::from(vec![
            Span::styled(language.workspace_title(), Style::default().fg(green)),
            Span::styled(
                language.workspace_thread_count(self.workspace.rows.len()),
                Style::default().fg(muted),
            ),
        ]);
        if framed {
            crate::surface::render_panel_surface(area, buf, theme, Some(title));
        } else if area.width > 2 && area.height > 0 {
            Paragraph::new(title)
                .style(Style::default().fg(text).bg(theme.panel_bg))
                .render(
                    Rect::new(
                        area.x.saturating_add(1),
                        area.y,
                        area.width.saturating_sub(2),
                        1,
                    ),
                    buf,
                );
        }

        if area.height < WORKSPACE_LIST_TOP_PADDING + 1 {
            self.workspace.toolbar_new_area = None;
            self.workspace.toolbar_search_area = None;
            return;
        }

        let (new_area, search_area) = workspace_toolbar_areas(area);
        self.workspace.toolbar_new_area = (!new_area.is_empty()).then_some(new_area);
        self.workspace.toolbar_search_area = (!search_area.is_empty()).then_some(search_area);
        let new_hovered =
            self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::StartThread);
        let search_hovered =
            self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::Search);
        let visual_palette = theme.visual_palette();
        let new_style = visual_palette.button_style(new_hovered, false);
        if !new_area.is_empty() {
            buf.set_style(new_area, new_style);
            Paragraph::new(Line::from(vec![Span::styled(
                workspace_truncate(language.workspace_new_thread(), new_area.width as usize),
                new_style.add_modifier(Modifier::BOLD),
            )]))
            .style(new_style)
            .render(new_area, buf);
        }
        if !search_area.is_empty() {
            let prefix = if self.workspace.search_focused {
                "> "
            } else {
                "/ "
            };
            let mut search_palette = ui_palette.clone();
            search_palette.surface_input =
                visual_palette.input_surface(self.workspace.search_focused, search_hovered);
            search_palette.accent_soft = if self.workspace.search_focused {
                visual_palette.control_accent
            } else {
                visual_palette.text_muted
            };
            search_palette.text = if self.workspace.search_focused {
                visual_palette.text_strong
            } else {
                visual_palette.text
            };
            search_palette.text_inactive = if self.workspace.search_focused {
                visual_palette.text_inactive
            } else {
                visual_palette.text_inactive
            };
            let cursor = InputVisual::new(self.workspace.search_query.as_str())
                .placeholder(language.workspace_search_placeholder())
                .prefix(prefix)
                .cursor_byte(self.workspace.search_query.len())
                .focused(self.workspace.search_focused)
                .render(search_area, buf, &search_palette);
            if let Some(cursor) = cursor
                && cursor.x < search_area.right()
            {
                buf[(cursor.x, cursor.y)].set_style(
                    Style::default()
                        .bg(search_palette.border_focused)
                        .fg(search_palette.text_inverse),
                );
            }
        }

        let active_thread_id = self.chat_widget.thread_id().or(self.active_thread_id);
        let max_rows = ((area.height - WORKSPACE_LIST_TOP_PADDING) / WORKSPACE_ROW_HEIGHT) as usize;
        let visible_items = self.workspace.visible_items();
        let item_count = visible_items.len() + usize::from(self.workspace.has_load_more_row());
        let first_visible_index = self.workspace.list_scroll().min(item_count);
        let last_visible_index = first_visible_index.saturating_add(max_rows).min(item_count);
        let text_width = area.width.saturating_sub(4) as usize;
        for (screen_index, visible_item_index) in
            (first_visible_index..last_visible_index).enumerate()
        {
            let y =
                area.y + WORKSPACE_LIST_TOP_PADDING + (screen_index as u16 * WORKSPACE_ROW_HEIGHT);
            let row_area = Rect::new(area.x + 1, y, area.width.saturating_sub(2), 2);
            if self.workspace.is_load_more_index(visible_item_index) {
                let is_selected = visible_item_index == self.workspace.selected;
                let is_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::LoadMore);
                let row_style = workspace_row_style(theme, false, is_selected, is_hovered, false);
                buf.set_style(row_area, row_style);
                render_workspace_row_accent(
                    buf,
                    row_area,
                    row_style,
                    workspace_row_accent(theme, false, is_selected, is_hovered, false),
                );
                let label = if self.workspace.is_loading_more() {
                    language.workspace_loading_more_threads()
                } else {
                    language.workspace_load_more_threads()
                };
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "+ ",
                        Style::default().fg(green).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(label, row_style.fg(green).add_modifier(Modifier::BOLD)),
                ]))
                .style(row_style)
                .render(
                    Rect::new(
                        row_area.x + 2,
                        row_area.y,
                        row_area.width.saturating_sub(4),
                        1,
                    ),
                    buf,
                );
                Paragraph::new(workspace_truncate(
                    &language.workspace_loaded_threads(self.workspace.rows.len()),
                    text_width.saturating_sub(2),
                ))
                .style(row_style.fg(muted))
                .render(
                    Rect::new(
                        row_area.x + 2,
                        row_area.y + 1,
                        row_area.width.saturating_sub(4),
                        1,
                    ),
                    buf,
                );
                continue;
            }
            let Some(visible_item) = visible_items.get(visible_item_index).copied() else {
                continue;
            };
            if let WorkspaceVisibleItem::ClosedSubagents { parent_index } = visible_item {
                let Some(parent_row) = self.workspace.rows.get(parent_index) else {
                    continue;
                };
                let is_selected = visible_item_index == self.workspace.selected;
                let is_hovered = matches!(
                    self.mouse.hover_workspace_target,
                    Some(WorkspaceMouseTarget::ClosedSubagentsToggle(target))
                        if target == parent_index
                );
                let row_style = workspace_row_style(theme, false, is_selected, is_hovered, false);
                buf.set_style(row_area, row_style);
                render_workspace_row_accent(
                    buf,
                    row_area,
                    row_style,
                    workspace_row_accent(theme, false, is_selected, is_hovered, false),
                );
                let expanded = self
                    .workspace
                    .expanded_closed_subagent_parent_ids
                    .contains(&parent_row.thread_id);
                let tree_prefix = if expanded { "▾" } else { "▸" };
                let row_indent = workspace_row_tree_indent(parent_row)
                    .saturating_add(WORKSPACE_SUBAGENT_INDENT_STEP)
                    .min(row_area.width.saturating_sub(2));
                let row_content_x = row_area.x.saturating_add(1).saturating_add(row_indent);
                let row_content_width = row_area.width.saturating_sub(2).saturating_sub(row_indent);
                let row_text_width = row_content_width as usize;
                let title_width = row_text_width.saturating_sub(WORKSPACE_STATUS_LABEL_WIDTH + 8);
                let title = workspace_truncate(
                    &workspace_closed_subagents_label(parent_row.subagents.closed, language),
                    title_width,
                );
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        tree_prefix,
                        Style::default().fg(muted).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(" ", Style::default().fg(muted)),
                    Span::raw(" "),
                    Span::styled(
                        format!(
                            "{:<width$}",
                            workspace_truncate("CLOSED", WORKSPACE_STATUS_LABEL_WIDTH),
                            width = WORKSPACE_STATUS_LABEL_WIDTH
                        ),
                        workspace_status_style(theme, muted, false),
                    ),
                    Span::raw(" "),
                    Span::styled(title, row_style.fg(muted).add_modifier(Modifier::BOLD)),
                ]))
                .style(row_style)
                .render(
                    Rect::new(row_content_x, row_area.y, row_content_width, 1),
                    buf,
                );
                Paragraph::new(workspace_truncate(
                    &workspace_closed_subagents_detail(parent_row.subagents.closed, language),
                    row_text_width,
                ))
                .style(row_style.fg(muted))
                .render(
                    Rect::new(row_content_x, row_area.y + 1, row_content_width, 1),
                    buf,
                );
                continue;
            }
            let WorkspaceVisibleItem::Thread(index) = visible_item else {
                continue;
            };
            let Some(row) = self.workspace.rows.get(index) else {
                continue;
            };
            let is_active = Some(row.thread_id) == active_thread_id;
            let is_selected = visible_item_index == self.workspace.selected;
            let is_hovered = self.mouse.hover_workspace_thread_index == Some(index)
                || matches!(
                    self.mouse.hover_workspace_target,
                    Some(WorkspaceMouseTarget::Thread(target)
                        | WorkspaceMouseTarget::SubagentsToggle(target)) if target == index
                );
            let is_controlled = workspace_row_is_controlled(row);
            let row_style =
                workspace_row_style(theme, is_active, is_selected, is_hovered, is_controlled);
            buf.set_style(row_area, row_style);
            render_workspace_row_accent(
                buf,
                row_area,
                row_style,
                workspace_row_accent(theme, is_active, is_selected, is_hovered, is_controlled),
            );

            let status_color = if is_controlled {
                controlled_accent
            } else {
                match &row.status {
                    ThreadStatus::Active { .. } => green,
                    ThreadStatus::Idle => muted,
                    ThreadStatus::SystemError => theme.danger,
                    ThreadStatus::NotLoaded => muted,
                }
            };
            let subagents_expanded = self
                .workspace
                .expanded_subagent_parent_ids
                .contains(&row.thread_id);
            let tree_prefix = workspace_row_tree_prefix(row, subagents_expanded);
            let subagent_marker = workspace_row_subagent_marker(row);
            let row_indent = workspace_row_tree_indent(row).min(row_area.width.saturating_sub(2));
            let row_content_x = row_area.x.saturating_add(1).saturating_add(row_indent);
            let row_content_width = row_area.width.saturating_sub(2).saturating_sub(row_indent);
            let row_text_width = row_content_width as usize;
            let subagent_marker_width = subagent_marker
                .as_ref()
                .map(|label| label.width().saturating_add(1))
                .unwrap_or(0);
            let tree_prefix_width = tree_prefix.width().saturating_add(1);
            let title_width = row_text_width.saturating_sub(
                WORKSPACE_STATUS_LABEL_WIDTH + 8 + tree_prefix_width + subagent_marker_width,
            );
            let pinned = self.workspace.pinned_thread_ids.contains(&row.thread_id);
            let pin = if pinned { "* " } else { "" };
            let title = workspace_truncate(
                &format!("{pin}{}", workspace_thread_display_name(row)),
                title_width,
            );
            let mut first_line_spans = vec![
                Span::styled(
                    tree_prefix,
                    Style::default()
                        .fg(if row.subagents.is_empty() {
                            muted
                        } else {
                            green
                        })
                        .add_modifier(if row.subagents.is_empty() {
                            Modifier::empty()
                        } else {
                            Modifier::BOLD
                        }),
                ),
                Span::raw(" "),
                Span::styled(
                    workspace_row_control_marker(row),
                    Style::default()
                        .fg(if is_controlled {
                            controlled_accent
                        } else {
                            muted
                        })
                        .add_modifier(if is_controlled {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::raw(" "),
                Span::styled(
                    format!(
                        "{:<width$}",
                        workspace_truncate(
                            &workspace_row_status_label(row),
                            WORKSPACE_STATUS_LABEL_WIDTH,
                        ),
                        width = WORKSPACE_STATUS_LABEL_WIDTH
                    ),
                    workspace_status_style(theme, status_color, is_controlled),
                ),
                Span::raw(" "),
                Span::styled(title, row_style),
            ];
            if let Some(subagent_marker) = subagent_marker {
                first_line_spans.push(Span::raw(" "));
                first_line_spans.push(Span::styled(
                    subagent_marker,
                    Style::default()
                        .fg(if is_controlled {
                            controlled_accent
                        } else {
                            green
                        })
                        .add_modifier(Modifier::BOLD),
                ));
            }
            let first_line = Line::from(first_line_spans);
            Paragraph::new(first_line).style(row_style).render(
                Rect::new(row_content_x, row_area.y, row_content_width, 1),
                buf,
            );

            let cwd_name = row
                .cwd
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| row.cwd.display().to_string());
            let usage = self
                .workspace
                .usage_by_thread
                .get(&row.thread_id)
                .or(row.token_usage.as_ref());
            let base_detail = match workspace_cache_label(usage, is_selected || is_hovered) {
                Some(cache_label) => {
                    format!(
                        "{}  {}  {}  {}",
                        cache_label,
                        row.source,
                        cwd_name,
                        row.preview.trim()
                    )
                }
                None => format!("{}  {}  {}", row.source, cwd_name, row.preview.trim()),
            };
            let detail = if let Some(control_state) = row.control_state.as_ref() {
                let control_banner = if control_state.read_only {
                    language.workspace_locked_view()
                } else {
                    language.workspace_controlled()
                };
                format!(
                    "{control_banner}  {}  {}",
                    workspace_control_detail(control_state, language),
                    base_detail
                )
            } else if is_controlled {
                format!("{}  {base_detail}", language.workspace_controlled())
            } else {
                base_detail
            };
            Paragraph::new(workspace_truncate(&detail, row_text_width))
                .style(row_style.fg(if is_controlled {
                    controlled_muted
                } else {
                    muted
                }))
                .render(
                    Rect::new(row_content_x, row_area.y + 1, row_content_width, 1),
                    buf,
                );
        }

        self.render_workspace_list_scrollbar(area, max_rows, buf);
        self.render_workspace_overlay(area, buf);
    }

    fn render_workspace_list_scrollbar(
        &self,
        area: Rect,
        visible_rows: usize,
        buf: &mut ratatui::buffer::Buffer,
    ) {
        let total = self.workspace.list_item_count();
        if visible_rows == 0 || total <= visible_rows || area.width == 0 {
            return;
        }

        let track_height = area.height.saturating_sub(WORKSPACE_LIST_TOP_PADDING);
        if track_height == 0 {
            return;
        }

        let theme = self.chat_widget.workspace_theme();
        let x = area.right().saturating_sub(1);
        let track_y = area.y.saturating_add(WORKSPACE_LIST_TOP_PADDING);
        let muted = theme.dim;
        let thumb = theme.accent;
        for offset in 0..track_height {
            buf[(x, track_y + offset)]
                .set_symbol("│")
                .set_style(Style::default().fg(muted).bg(theme.panel_bg));
        }

        let thumb_height = ((visible_rows * track_height as usize) / total)
            .max(1)
            .min(track_height as usize) as u16;
        let max_scroll = total.saturating_sub(visible_rows).max(1);
        let thumb_offset = ((self.workspace.list_scroll()
            * track_height.saturating_sub(thumb_height) as usize)
            / max_scroll) as u16;
        for offset in 0..thumb_height {
            buf[(x, track_y + thumb_offset + offset)]
                .set_symbol("┃")
                .set_style(Style::default().fg(thumb).bg(theme.panel_bg));
        }
    }

    fn render_workspace_overlay(&mut self, list_area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let overlay = self.workspace.overlay.clone();
        let language = self.chat_widget.ui_language();
        let theme = self.chat_widget.workspace_theme();
        match overlay {
            WorkspaceOverlay::None
            | WorkspaceOverlay::ChromeMenu(_)
            | WorkspaceOverlay::OpenFolder(_) => {}
            WorkspaceOverlay::ContextMenu(menu) => {
                let actions = workspace_menu_actions();
                let row = self
                    .workspace
                    .rows
                    .iter()
                    .find(|row| row.thread_id == menu.thread_id);
                let subagent_lines = workspace_context_subagent_lines(row, language);
                let area = workspace_popup_area(
                    list_area,
                    menu.anchor_column,
                    menu.anchor_row,
                    WORKSPACE_CONTEXT_MENU_WIDTH,
                    actions.len() as u16 + subagent_lines.len() as u16 + 2,
                );
                if let WorkspaceOverlay::ContextMenu(current) = &mut self.workspace.overlay {
                    current.area = Some(area);
                }
                let panel = theme.dropdown_bg;
                let selected_bg = theme.selected_bg;
                let text = theme.text;
                let muted = theme.muted;
                let green = theme.accent;
                crate::surface::render_popup_surface(
                    area,
                    buf,
                    theme,
                    Some(Line::from(language.workspace_context_title())),
                );
                let pinned = self.workspace.pinned_thread_ids.contains(&menu.thread_id);
                let locked = self
                    .workspace
                    .rows
                    .iter()
                    .find(|row| row.thread_id == menu.thread_id)
                    .is_some_and(workspace_row_is_controlled);
                for (index, action) in actions.iter().copied().enumerate() {
                    let y = area.y.saturating_add(1 + index as u16);
                    let is_selected = index == menu.selected
                        || self.mouse.hover_workspace_target
                            == Some(WorkspaceMouseTarget::ContextMenu(action));
                    let disabled = workspace_menu_action_disabled(action, locked);
                    let style = if is_selected && disabled {
                        Style::default().bg(selected_bg).fg(muted)
                    } else if is_selected {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else if disabled {
                        Style::default().bg(panel).fg(muted)
                    } else if matches!(
                        action,
                        WorkspaceMenuAction::Archive | WorkspaceMenuAction::Delete
                    ) {
                        Style::default().bg(panel).fg(theme.danger)
                    } else if matches!(action, WorkspaceMenuAction::TogglePin) && pinned {
                        Style::default().bg(panel).fg(green)
                    } else {
                        Style::default().bg(panel).fg(text)
                    };
                    Paragraph::new(workspace_menu_action_label(
                        action, pinned, locked, language,
                    ))
                    .style(style)
                    .render(
                        Rect::new(area.x.saturating_add(1), y, area.width.saturating_sub(2), 1),
                        buf,
                    );
                }
                let details_y = area.y.saturating_add(1 + actions.len() as u16);
                for (line_index, line) in subagent_lines.iter().enumerate() {
                    let y = details_y.saturating_add(line_index as u16);
                    if y >= area.bottom().saturating_sub(1) {
                        break;
                    }
                    let style = if line_index == 0 {
                        Style::default().bg(panel).fg(green)
                    } else {
                        Style::default().bg(panel).fg(muted)
                    };
                    Paragraph::new(workspace_truncate(
                        line,
                        area.width.saturating_sub(2) as usize,
                    ))
                    .style(style)
                    .render(
                        Rect::new(area.x.saturating_add(1), y, area.width.saturating_sub(2), 1),
                        buf,
                    );
                }
            }
            WorkspaceOverlay::Rename(rename) => {
                let area = workspace_dialog_area(list_area, WORKSPACE_RENAME_POPUP_WIDTH, 6);
                if let WorkspaceOverlay::Rename(current) = &mut self.workspace.overlay {
                    current.area = Some(area);
                }
                let panel = theme.dropdown_bg;
                let selected_bg = theme.selected_bg;
                let text = theme.text;
                let muted = theme.muted;
                let green = theme.accent;
                crate::surface::render_popup_surface(
                    area,
                    buf,
                    theme,
                    Some(Line::from(language.workspace_rename_title())),
                );
                Paragraph::new(language.workspace_thread_name_label())
                    .style(Style::default().bg(panel).fg(muted))
                    .render(Rect::new(area.x + 2, area.y + 1, area.width - 4, 1), buf);
                let mut value = rename.value.clone();
                let cursor = rename.cursor.min(value.len());
                value.insert(cursor, '|');
                Paragraph::new(workspace_truncate(
                    &value,
                    area.width.saturating_sub(4) as usize,
                ))
                .style(Style::default().bg(theme.input_bg).fg(theme.text_strong))
                .render(Rect::new(area.x + 2, area.y + 2, area.width - 4, 1), buf);
                let save_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::RenameSave);
                let cancel_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::RenameCancel);
                Paragraph::new(language.workspace_save_label())
                    .style(if save_hovered {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(panel).fg(green)
                    })
                    .render(Rect::new(area.x + 2, area.y + 4, 8, 1), buf);
                Paragraph::new(language.workspace_cancel_label())
                    .style(if cancel_hovered {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(panel).fg(text)
                    })
                    .render(Rect::new(area.x + 12, area.y + 4, 10, 1), buf);
            }
            WorkspaceOverlay::ConfirmArchive(confirm) => {
                let area = workspace_dialog_area(list_area, WORKSPACE_CONFIRM_POPUP_WIDTH, 5);
                if let WorkspaceOverlay::ConfirmArchive(current) = &mut self.workspace.overlay {
                    current.area = Some(area);
                }
                let panel = theme.dropdown_bg;
                let selected_bg = theme.selected_bg;
                let text = theme.text;
                let danger = theme.danger;
                crate::surface::render_popup_surface(
                    area,
                    buf,
                    theme,
                    Some(Line::from(language.workspace_archive_title())),
                );
                let name = self
                    .workspace
                    .rows
                    .iter()
                    .find(|row| row.thread_id == confirm.thread_id)
                    .map(|row| row.name.as_str())
                    .unwrap_or(match language {
                        UiLanguage::En => "this thread",
                        UiLanguage::Cn => "此线程",
                    });
                Paragraph::new(workspace_truncate(
                    &language.workspace_archive_prompt(name),
                    area.width.saturating_sub(4) as usize,
                ))
                .style(Style::default().bg(panel).fg(text))
                .render(Rect::new(area.x + 2, area.y + 1, area.width - 4, 1), buf);
                let confirm_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::ArchiveConfirm);
                let cancel_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::ArchiveCancel);
                Paragraph::new(language.workspace_archive_title())
                    .style(if confirm_hovered {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(panel).fg(danger)
                    })
                    .render(Rect::new(area.x + 2, area.y + 3, 10, 1), buf);
                Paragraph::new(language.workspace_cancel_label())
                    .style(if cancel_hovered {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(panel).fg(text)
                    })
                    .render(Rect::new(area.x + 14, area.y + 3, 10, 1), buf);
            }
            WorkspaceOverlay::ConfirmDelete(confirm) => {
                let area = workspace_dialog_area(list_area, WORKSPACE_CONFIRM_POPUP_WIDTH, 5);
                if let WorkspaceOverlay::ConfirmDelete(current) = &mut self.workspace.overlay {
                    current.area = Some(area);
                }
                let panel = theme.dropdown_bg;
                let selected_bg = theme.selected_bg;
                let text = theme.text;
                let danger = theme.danger;
                crate::surface::render_popup_surface(
                    area,
                    buf,
                    theme,
                    Some(Line::from(language.workspace_delete_title())),
                );
                let name = self
                    .workspace
                    .rows
                    .iter()
                    .find(|row| row.thread_id == confirm.thread_id)
                    .map(|row| row.name.as_str())
                    .unwrap_or(match language {
                        UiLanguage::En => "this thread",
                        UiLanguage::Cn => "此线程",
                    });
                Paragraph::new(workspace_truncate(
                    &language.workspace_delete_prompt(name),
                    area.width.saturating_sub(4) as usize,
                ))
                .style(Style::default().bg(panel).fg(text))
                .render(Rect::new(area.x + 2, area.y + 1, area.width - 4, 1), buf);
                let confirm_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::DeleteConfirm);
                let cancel_hovered =
                    self.mouse.hover_workspace_target == Some(WorkspaceMouseTarget::DeleteCancel);
                Paragraph::new(language.workspace_delete_title())
                    .style(if confirm_hovered {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(panel).fg(danger)
                    })
                    .render(Rect::new(area.x + 2, area.y + 3, 10, 1), buf);
                Paragraph::new(language.workspace_cancel_label())
                    .style(if cancel_hovered {
                        Style::default().bg(selected_bg).fg(theme.text_strong)
                    } else {
                        Style::default().bg(panel).fg(text)
                    })
                    .render(Rect::new(area.x + 14, area.y + 3, 10, 1), buf);
            }
        }
    }
}
