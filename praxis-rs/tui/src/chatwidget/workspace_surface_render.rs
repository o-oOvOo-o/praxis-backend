use std::sync::Arc;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::text::Text;
use ratatui::widgets::Block;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use super::CHAT_SURFACE_CONTENT_MAX_WIDTH;
use super::ChatWidget;
use super::DEEPSEEK_CHROME_MIN_HEIGHT;
use super::DEEPSEEK_FOOTER_HEIGHT;
use super::DEEPSEEK_HEADER_HEIGHT;
use super::WORKSPACE_ENTRY_INTRO_HEIGHT;
use super::WORKSPACE_ENTRY_MAX_WIDTH;
use super::WORKSPACE_ENTRY_MIN_SIDE_PADDING;
use super::WORKSPACE_INPUT_BORDER_COLS;
use super::WORKSPACE_INPUT_BORDER_ROWS;
use super::WORKSPACE_INPUT_STRIP_ROWS;
use super::surface_layout::ChatSurfaceLayoutInput;
use super::surface_layout::ChatWidgetLayout;
use super::surface_layout::IN_APP_TOAST_ROW_HEIGHT;
use super::surface_layout::chat_surface_split_for_width;
use super::surface_layout::layout_chat_surface;
use super::thread_control_display_label;
use crate::history_cell::HistoryCell;
use crate::line_truncation::line_width;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;
use crate::toast_queue::ToastEntry;
use crate::toast_queue::ToastSeverity;
use crate::ui_language::UiLanguage;
use crate::workspace::LaunchStripState;

impl Renderable for ChatWidget {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let launch = LaunchStripState::default();
        self.render_chat_surface(area, buf, &[], 0, &launch, true);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.desired_total_height(width)
            .saturating_add(Self::deepseek_chrome_height_for_width(width))
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.workspace_cursor_pos(area)
    }
}

impl ChatWidget {
    pub(crate) fn render_standalone_chat(
        &self,
        area: Rect,
        buf: &mut Buffer,
        transcript_cells: &[Arc<dyn HistoryCell>],
        scroll_from_bottom: usize,
        launch: &LaunchStripState,
    ) {
        self.render_chat_surface(
            area,
            buf,
            transcript_cells,
            scroll_from_bottom,
            launch,
            true,
        );
    }

    pub(crate) fn render_workspace_chat_embedded(
        &self,
        area: Rect,
        buf: &mut Buffer,
        transcript_cells: &[Arc<dyn HistoryCell>],
        scroll_from_bottom: usize,
        launch: &LaunchStripState,
    ) {
        self.render_chat_surface(
            area,
            buf,
            transcript_cells,
            scroll_from_bottom,
            launch,
            false,
        );
    }

    fn render_chat_surface(
        &self,
        area: Rect,
        buf: &mut Buffer,
        transcript_cells: &[Arc<dyn HistoryCell>],
        scroll_from_bottom: usize,
        launch: &LaunchStripState,
        framed: bool,
    ) {
        let theme = self.workspace_theme();
        if framed {
            self.render_deepseek_background(area, buf);
        } else {
            buf.set_style(
                area,
                Style::default().bg(theme.panel_raised_bg).fg(theme.text),
            );
        }
        let chrome_area = if framed {
            Self::workspace_surface_inner_area(area)
        } else {
            area
        };
        let header_area = Self::surface_header_area(chrome_area);
        let footer_area = Self::surface_footer_area(chrome_area);
        let body_area = Self::surface_body_area(chrome_area);

        if let Some(header_area) = header_area {
            self.render_deepseek_header(header_area, buf);
        }

        let use_entry_layout = self.workspace_entry_state_visible(transcript_cells)
            && !self.bottom_pane.has_active_view();
        let layout = if use_entry_layout {
            self.workspace_entry_layout_for_area(body_area)
        } else {
            self.workspace_layout_for_area(body_area)
        };
        if use_entry_layout {
            self.replace_visible_patch_cell_ids(Vec::new());
            self.render_workspace_entry_intro(layout, buf);
        } else if let Some(viewport) =
            self.workspace_transcript_viewport(layout, transcript_cells, scroll_from_bottom)
        {
            self.replace_visible_patch_cell_ids(viewport.patch_cell_ids.clone());
            self.render_workspace_transcript_viewport(&viewport, buf);
        } else {
            self.replace_visible_patch_cell_ids(Vec::new());
        }
        self.render_work_panel(layout, buf);
        self.render_bottom_pane(layout, buf);
        self.render_launch_strip_strip(layout, buf, launch);
        self.render_launch_strip_dropdown(layout, buf, launch);
        self.render_in_app_toast_overlay(layout, buf);

        if let Some(footer_area) = footer_area {
            self.render_deepseek_footer(footer_area, buf);
        }
        if framed {
            crate::surface::render_panel_outline(
                area,
                buf,
                theme,
                Some(Line::from(vec![
                    Span::styled(
                        " Praxis ",
                        Style::default()
                            .fg(theme.title_fg)
                            .bg(theme.panel_bg)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        self.model_display_name().to_string(),
                        Style::default().fg(theme.muted).bg(theme.panel_bg),
                    ),
                ])),
            );
        }
        self.last_rendered_width.set(Some(area.width as usize));
    }

    pub(crate) fn workspace_cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let body_area = self.deepseek_body_area(area);
        self.workspace_cursor_pos_for_body(body_area)
    }

    pub(crate) fn workspace_cursor_pos_embedded(&self, area: Rect) -> Option<(u16, u16)> {
        self.workspace_cursor_pos_for_body(Self::surface_body_area(area))
    }

    fn workspace_cursor_pos_for_body(&self, body_area: Rect) -> Option<(u16, u16)> {
        let layout = if self.workspace_entry_state_active() && !self.bottom_pane.has_active_view() {
            self.workspace_entry_layout_for_area(body_area)
        } else {
            self.workspace_layout_for_area(body_area)
        };
        if layout.bottom_content_area.is_empty() {
            None
        } else {
            self.bottom_pane.cursor_pos(layout.bottom_content_area)
        }
    }

    pub(crate) fn visible_patch_cell_ids(&self) -> Vec<crate::history_presentation::PatchCellId> {
        self.last_visible_patch_cell_ids.borrow().clone()
    }

    fn replace_visible_patch_cell_ids(&self, ids: Vec<crate::history_presentation::PatchCellId>) {
        *self.last_visible_patch_cell_ids.borrow_mut() = ids;
    }

    pub(crate) fn workspace_chat_scroll_limit(
        &self,
        area: Rect,
        transcript_cells: &[Arc<dyn HistoryCell>],
        current_scroll_from_bottom: usize,
    ) -> usize {
        if self.workspace_entry_state_visible(transcript_cells) {
            return 0;
        }
        let body_area = self.deepseek_body_area(area);
        self.workspace_chat_scroll_limit_for_body(
            body_area,
            transcript_cells,
            current_scroll_from_bottom,
        )
    }

    pub(crate) fn workspace_chat_scroll_limit_embedded(
        &self,
        area: Rect,
        transcript_cells: &[Arc<dyn HistoryCell>],
        current_scroll_from_bottom: usize,
    ) -> usize {
        if self.workspace_entry_state_visible(transcript_cells) {
            return 0;
        }
        self.workspace_chat_scroll_limit_for_body(
            Self::surface_body_area(area),
            transcript_cells,
            current_scroll_from_bottom,
        )
    }

    fn workspace_chat_scroll_limit_for_body(
        &self,
        body_area: Rect,
        transcript_cells: &[Arc<dyn HistoryCell>],
        _current_scroll_from_bottom: usize,
    ) -> usize {
        let layout = self.workspace_layout_for_area(body_area);
        let Some(content_area) = layout.active_content_area else {
            return 0;
        };
        if content_area.is_empty() {
            return 0;
        }

        self.workspace_transcript_scroll_limit(content_area, transcript_cells)
    }

    fn deepseek_chrome_enabled(area: Rect) -> bool {
        area.height >= DEEPSEEK_CHROME_MIN_HEIGHT
    }

    fn deepseek_chrome_height_for_width(_width: u16) -> u16 {
        DEEPSEEK_HEADER_HEIGHT.saturating_add(DEEPSEEK_FOOTER_HEIGHT)
    }

    fn deepseek_header_area(&self, area: Rect) -> Option<Rect> {
        Self::surface_header_area(Self::workspace_surface_inner_area(area))
    }

    fn deepseek_footer_area(&self, area: Rect) -> Option<Rect> {
        Self::surface_footer_area(Self::workspace_surface_inner_area(area))
    }

    fn deepseek_body_area(&self, area: Rect) -> Rect {
        Self::surface_body_area(Self::workspace_surface_inner_area(area))
    }

    fn surface_header_area(area: Rect) -> Option<Rect> {
        (Self::deepseek_chrome_enabled(area) && !area.is_empty()).then_some(Rect::new(
            area.x,
            area.y,
            area.width,
            DEEPSEEK_HEADER_HEIGHT.min(area.height),
        ))
    }

    fn surface_footer_area(area: Rect) -> Option<Rect> {
        (Self::deepseek_chrome_enabled(area) && !area.is_empty()).then_some(Rect::new(
            area.x,
            area.bottom().saturating_sub(DEEPSEEK_FOOTER_HEIGHT),
            area.width,
            DEEPSEEK_FOOTER_HEIGHT.min(area.height),
        ))
    }

    fn surface_body_area(area: Rect) -> Rect {
        if !Self::deepseek_chrome_enabled(area) {
            return area;
        }
        Rect::new(
            area.x,
            area.y.saturating_add(DEEPSEEK_HEADER_HEIGHT),
            area.width,
            area.height
                .saturating_sub(DEEPSEEK_HEADER_HEIGHT)
                .saturating_sub(DEEPSEEK_FOOTER_HEIGHT),
        )
    }

    fn render_deepseek_background(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.workspace_theme();
        crate::surface::render_panel_surface(area, buf, theme, None);
    }

    fn workspace_surface_inner_area(area: Rect) -> Rect {
        if area.width <= 2 || area.height <= 2 {
            return area;
        }
        Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        )
    }

    fn render_deepseek_header(&self, area: Rect, buf: &mut Buffer) {
        self.render_deepseek_header_with_hover(area, buf, false);
    }

    fn render_deepseek_header_with_hover(&self, area: Rect, buf: &mut Buffer, model_hovered: bool) {
        if area.is_empty() {
            return;
        }
        let theme = self.workspace_theme();
        Block::default()
            .style(Style::default().bg(theme.header_bg))
            .render(area, buf);

        let workspace = self
            .config
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("workspace");
        let thread = self
            .thread_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("new thread");
        let mode = self.collaboration_mode_label().unwrap_or("Default");
        let provider = self.current_model_provider_id();
        let budget = self.status_budget_message();
        let model_style = if model_hovered {
            Style::default()
                .fg(theme.accent)
                .bg(theme.hover_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };
        let provider_style = if model_hovered {
            Style::default().fg(theme.accent).bg(theme.hover_bg)
        } else {
            Style::default().fg(theme.muted)
        };

        let mut spans = vec![
            Span::styled(
                " Praxis ",
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.header_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(workspace.to_string(), Style::default().fg(theme.text)),
            Span::styled("  ", Style::default().bg(theme.header_bg)),
            Span::styled(mode.to_string(), Style::default().fg(theme.control_accent)),
            Span::styled("  ", Style::default().bg(theme.header_bg)),
            Span::styled(self.model_display_name().to_string(), model_style),
            Span::styled(format!(" ({provider})"), provider_style),
            Span::styled("  ", Style::default().bg(theme.header_bg)),
            Span::styled(thread.to_string(), Style::default().fg(theme.muted)),
        ];
        if let Some(control_state) = self.thread_control_state.as_ref() {
            spans.push(Span::styled("  ", Style::default().bg(theme.header_bg)));
            spans.push(Span::styled(
                format!("Locked by {}", thread_control_display_label(control_state)),
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.header_bg)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(budget) = budget {
            spans.push(Span::styled(
                format!("  {budget}"),
                Style::default().fg(theme.muted),
            ));
        }

        let line = truncate_line_with_ellipsis_if_overflow(
            Line::from(spans).style(Style::default().bg(theme.header_bg)),
            area.width as usize,
        );
        Paragraph::new(line)
            .style(Style::default().bg(theme.header_bg))
            .render(area, buf);
    }

    fn render_deepseek_footer(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.workspace_theme();
        Block::default()
            .style(Style::default().bg(theme.footer_bg))
            .render(area, buf);

        let running = self.bottom_pane.is_task_running();
        let locked_label = self.thread_control_state.as_ref().map(|control_state| {
            format!("Locked by {}", thread_control_display_label(control_state))
        });
        let state_is_locked = locked_label.is_some();
        let state_label = locked_label.as_deref().unwrap_or(if running {
            self.current_status.header.as_str()
        } else {
            "Ready"
        });
        let queued_count = self
            .queued_user_messages
            .len()
            .saturating_add(self.rejected_steers_queue.len())
            .saturating_add(self.pending_steers.len());
        let mut spans = vec![
            Span::styled(
                format!(" {state_label}"),
                Style::default()
                    .fg(if state_is_locked || running {
                        theme.accent
                    } else {
                        theme.muted
                    })
                    .bg(theme.footer_bg)
                    .add_modifier(if state_is_locked || running {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled("  ", Style::default().bg(theme.footer_bg)),
            Span::styled(
                if state_is_locked {
                    "controlled live view | input locked"
                } else if running {
                    "Esc interrupt | type to steer"
                } else {
                    "Enter send | Shift+Enter newline"
                },
                Style::default().fg(theme.muted).bg(theme.footer_bg),
            ),
        ];
        if queued_count > 0 {
            spans.push(Span::styled(
                format!("  queued {queued_count}"),
                Style::default()
                    .fg(theme.control_accent)
                    .bg(theme.footer_bg),
            ));
        }
        let line = truncate_line_with_ellipsis_if_overflow(
            Line::from(spans).style(Style::default().bg(theme.footer_bg)),
            area.width as usize,
        );
        Paragraph::new(line)
            .style(Style::default().bg(theme.footer_bg))
            .render(area, buf);
    }

    fn workspace_layout_for_area(&self, area: Rect) -> ChatWidgetLayout {
        let split = chat_surface_split_for_width(area.width, self.work_panel.should_show());
        let content_width = Self::chat_surface_column_width(split.agent_width);
        let work_panel_outer_height = split
            .work_panel_width
            .map(|panel_width| self.work_panel.desired_height(panel_width))
            .unwrap_or(0);
        let visible_toasts = u16::try_from(self.in_app_toasts.visible_entries().len())
            .unwrap_or(u16::MAX)
            .saturating_mul(IN_APP_TOAST_ROW_HEIGHT);
        let layout = layout_chat_surface(ChatSurfaceLayoutInput {
            area,
            agent_outer_height: area.height,
            bottom_outer_height: self.bottom_pane_total_height(content_width),
            toast_height: visible_toasts,
            work_panel_outer_height,
            show_work_panel: self.work_panel.should_show(),
            fill_available_top: true,
        });
        Self::workspace_bottom_pane_layout(layout)
    }

    fn chat_surface_column_width(width: u16) -> u16 {
        if width == 0 {
            0
        } else {
            width.min(CHAT_SURFACE_CONTENT_MAX_WIDTH).max(1)
        }
    }

    fn workspace_chat_column_rect(area: Rect) -> Rect {
        if area.is_empty() {
            return area;
        }
        let width = Self::chat_surface_column_width(area.width);
        Rect::new(
            area.x.saturating_add(area.width.saturating_sub(width) / 2),
            area.y,
            width,
            area.height,
        )
    }

    fn workspace_input_inner_area(area: Rect) -> Rect {
        if area.width <= WORKSPACE_INPUT_BORDER_COLS || area.height <= WORKSPACE_INPUT_BORDER_ROWS {
            return Rect::new(area.x, area.y, 0, 0);
        }
        Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(WORKSPACE_INPUT_BORDER_COLS),
            area.height.saturating_sub(WORKSPACE_INPUT_BORDER_ROWS),
        )
    }

    pub(super) fn workspace_input_strip_area(area: Rect) -> Rect {
        let inner = Self::workspace_input_inner_area(area);
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            WORKSPACE_INPUT_STRIP_ROWS.min(inner.height),
        )
    }

    fn workspace_input_composer_area(area: Rect) -> Rect {
        let inner = Self::workspace_input_inner_area(area);
        let strip_height = WORKSPACE_INPUT_STRIP_ROWS.min(inner.height);
        Rect::new(
            inner.x,
            inner.y.saturating_add(strip_height),
            inner.width,
            inner.height.saturating_sub(strip_height),
        )
    }

    fn workspace_bottom_pane_layout(mut layout: ChatWidgetLayout) -> ChatWidgetLayout {
        if layout.bottom_outer_area.is_empty() {
            return layout;
        }

        let bottom_area = layout.bottom_outer_area;
        let bottom_base_area = layout
            .active_outer_area
            .map(|active| Rect::new(active.x, bottom_area.y, active.width, bottom_area.height))
            .unwrap_or(bottom_area);
        let bottom_outer_area = Self::workspace_chat_column_rect(bottom_base_area);
        let bottom_content_area = Self::workspace_input_composer_area(bottom_outer_area);
        layout.bottom_outer_area = bottom_outer_area;
        layout.bottom_content_area = bottom_content_area;
        layout
    }

    fn workspace_entry_layout_for_area(&self, area: Rect) -> ChatWidgetLayout {
        if area.is_empty() {
            return ChatWidgetLayout::default();
        }

        let content_width = if area.width <= WORKSPACE_ENTRY_MIN_SIDE_PADDING.saturating_mul(2) {
            area.width
        } else {
            area.width
                .saturating_sub(WORKSPACE_ENTRY_MIN_SIDE_PADDING.saturating_mul(2))
                .min(WORKSPACE_ENTRY_MAX_WIDTH)
                .max(1)
        };
        let x = area
            .x
            .saturating_add(area.width.saturating_sub(content_width) / 2);
        let bottom_outer_height = self
            .bottom_pane_total_height(content_width)
            .min(area.height);
        let intro_height =
            WORKSPACE_ENTRY_INTRO_HEIGHT.min(area.height.saturating_sub(bottom_outer_height));
        let toast_height = u16::try_from(self.in_app_toasts.visible_entries().len())
            .unwrap_or(u16::MAX)
            .saturating_mul(IN_APP_TOAST_ROW_HEIGHT)
            .min(
                area.height
                    .saturating_sub(bottom_outer_height)
                    .saturating_sub(intro_height),
            );
        let bottom_margin = 1.min(area.height.saturating_sub(bottom_outer_height));
        let bottom_y = area
            .bottom()
            .saturating_sub(bottom_margin)
            .saturating_sub(bottom_outer_height);
        let intro_gap = 2.min(bottom_y.saturating_sub(area.y));
        let intro_y = bottom_y
            .saturating_sub(intro_gap)
            .saturating_sub(intro_height);

        let active_outer_area =
            (intro_height > 0).then_some(Rect::new(x, intro_y, content_width, intro_height));
        let active_content_area = active_outer_area;
        let toast_area = (toast_height > 0).then_some(Rect::new(
            x,
            bottom_y.saturating_sub(toast_height),
            content_width,
            toast_height,
        ));
        let bottom_outer_available = area.bottom().saturating_sub(bottom_y);
        let bottom_outer_area = Rect::new(
            x,
            bottom_y,
            content_width,
            bottom_outer_height.min(bottom_outer_available),
        );
        let bottom_content_area = Self::workspace_input_composer_area(bottom_outer_area);

        ChatWidgetLayout {
            active_outer_area,
            active_content_area,
            work_panel_area: None,
            toast_area,
            bottom_outer_area,
            bottom_content_area,
        }
    }

    fn workspace_entry_state_visible(&self, transcript_cells: &[Arc<dyn HistoryCell>]) -> bool {
        transcript_cells.is_empty() && self.workspace_entry_state_active()
    }

    fn workspace_entry_state_active(&self) -> bool {
        self.thread_id.is_none() && self.active_cell.is_none()
    }

    fn render_workspace_entry_intro(&self, layout: ChatWidgetLayout, buf: &mut Buffer) {
        let Some(area) = layout.active_content_area else {
            return;
        };
        if area.is_empty() {
            return;
        }

        let workspace = self
            .config
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("workspace");
        let title = match self.ui_language {
            UiLanguage::En => format!("What should Praxis do in {workspace}?"),
            UiLanguage::Cn => format!("在 {workspace} 中要做什么？"),
        };
        let subtitle = match self.ui_language {
            UiLanguage::En => "New coordinator thread in this workspace.",
            UiLanguage::Cn => "此工作区的新协调线程。",
        };
        let theme = self.workspace_theme();
        let cat = self.workspace_cat_frame();
        let lines = vec![
            Line::from(Span::styled(cat[0], Style::default().fg(theme.accent))),
            Line::from(Span::styled(cat[1], Style::default().fg(theme.accent))),
            Line::from(Span::styled(cat[2], Style::default().fg(theme.accent))),
            Line::from(""),
            Line::from(Span::styled(
                title,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(subtitle, Style::default().fg(theme.muted))),
        ];
        Paragraph::new(Text::from(lines))
            .alignment(Alignment::Center)
            .render(area, buf);
    }

    fn workspace_cat_frame(&self) -> [&'static str; 3] {
        if !self.tui_config.animations {
            return [" /\\_/\\ ", "( o.o )", " > ^ < "];
        }
        let frame = Instant::now()
            .duration_since(self.terminal_title_animation_origin)
            .as_millis()
            / 700
            % 3;
        match frame {
            1 => [" /\\_/\\ ", "( -.- )", " > ^ < "],
            2 => [" /\\_/\\ ", "( o.o )", "  / \\  "],
            _ => [" /\\_/\\ ", "( o.o )", " > ^ < "],
        }
    }

    fn bottom_pane_total_height(&self, width: u16) -> u16 {
        let composer_width = width.saturating_sub(WORKSPACE_INPUT_BORDER_COLS);
        self.bottom_pane
            .desired_height(composer_width)
            .saturating_add(WORKSPACE_INPUT_STRIP_ROWS)
            .saturating_add(WORKSPACE_INPUT_BORDER_ROWS)
    }

    fn desired_total_height(&self, width: u16) -> u16 {
        let split = chat_surface_split_for_width(width, self.work_panel.should_show());
        let content_width = Self::chat_surface_column_width(split.agent_width);
        let work_panel_height = split
            .work_panel_width
            .map(|panel_width| self.work_panel.desired_height(panel_width))
            .unwrap_or(0);
        let toast_height = u16::try_from(self.in_app_toasts.visible_entries().len())
            .unwrap_or(u16::MAX)
            .saturating_mul(IN_APP_TOAST_ROW_HEIGHT);
        self.active_cell_total_height(split.agent_width)
            .max(work_panel_height)
            .saturating_add(toast_height)
            .saturating_add(self.bottom_pane_total_height(content_width))
    }

    pub(super) fn layout_for_area(&self, area: Rect) -> ChatWidgetLayout {
        let split = chat_surface_split_for_width(area.width, self.work_panel.should_show());
        let content_width = Self::chat_surface_column_width(split.agent_width);
        let work_panel_outer_height = split
            .work_panel_width
            .map(|panel_width| self.work_panel.desired_height(panel_width))
            .unwrap_or(0);
        let visible_toasts = u16::try_from(self.in_app_toasts.visible_entries().len())
            .unwrap_or(u16::MAX)
            .saturating_mul(IN_APP_TOAST_ROW_HEIGHT);
        let layout = layout_chat_surface(ChatSurfaceLayoutInput {
            area,
            agent_outer_height: self.active_cell_total_height(split.agent_width),
            bottom_outer_height: self.bottom_pane_total_height(content_width),
            toast_height: visible_toasts,
            work_panel_outer_height,
            show_work_panel: self.work_panel.should_show(),
            fill_available_top: false,
        });
        Self::workspace_bottom_pane_layout(layout)
    }

    fn render_work_panel(&self, layout: ChatWidgetLayout, buf: &mut Buffer) {
        let Some(area) = layout.work_panel_area else {
            return;
        };
        let card_area = if area.width > 1 && area.height > 1 {
            Rect::new(area.x, area.y, area.width - 1, area.height - 1)
        } else {
            area
        };
        self.work_panel
            .render(card_area, buf, self.workspace_theme());
    }

    fn render_bottom_pane(&self, layout: ChatWidgetLayout, buf: &mut Buffer) {
        if layout.bottom_outer_area.is_empty() {
            return;
        }
        self.render_workspace_input_frame(layout.bottom_outer_area, buf);
        if layout.bottom_content_area.is_empty() {
            return;
        }
        self.bottom_pane.render(layout.bottom_content_area, buf);
    }

    fn render_workspace_input_frame(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.workspace_theme();
        buf.set_style(
            area,
            Style::default().bg(theme.panel_raised_bg).fg(theme.text),
        );
    }

    fn render_in_app_toast_overlay(&self, layout: ChatWidgetLayout, buf: &mut Buffer) {
        let Some(toast_area) = layout.toast_area else {
            return;
        };
        if toast_area.is_empty() {
            return;
        }

        for (index, toast) in self.in_app_toasts.visible_entries().into_iter().enumerate() {
            let Ok(offset) = u16::try_from(index) else {
                break;
            };
            let row = toast_area.y.saturating_add(offset);
            if row >= toast_area.bottom() {
                break;
            }
            InAppToastRenderable::new(toast).render(
                Rect::new(toast_area.x, row, toast_area.width, IN_APP_TOAST_ROW_HEIGHT),
                buf,
            );
        }
    }
}

struct InAppToastRenderable<'a> {
    toast: &'a ToastEntry,
}

impl<'a> InAppToastRenderable<'a> {
    fn new(toast: &'a ToastEntry) -> Self {
        Self { toast }
    }
}

impl Renderable for InAppToastRenderable<'_> {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let (label, accent) = match self.toast.severity {
            ToastSeverity::Info => ("Info", Style::default().cyan().bold()),
            ToastSeverity::Notice => ("Notice", Style::default().yellow().bold()),
            ToastSeverity::Error => ("Error", Style::default().red().bold()),
        };
        let mut line = truncate_line_with_ellipsis_if_overflow(
            Line::from(vec![
                Span::styled("▸ ", accent),
                Span::styled(label, accent),
                " ".into(),
                Span::styled(self.toast.message.clone(), Style::default().bold()),
            ]),
            usize::from(area.width),
        );
        let used_width = line_width(&line);
        if used_width < usize::from(area.width) {
            line.spans
                .push(Span::raw(" ".repeat(usize::from(area.width) - used_width)));
        }
        Paragraph::new(line).render(area, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        1
    }
}
