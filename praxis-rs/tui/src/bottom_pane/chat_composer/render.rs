use super::*;

impl Renderable for ChatComposer {
    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if !self.input_enabled || self.selected_remote_image_index.is_some() {
            return None;
        }

        let [_, _, textarea_rect, _] = self.layout_areas(area);
        let state = *self.textarea_state.borrow();
        self.textarea.cursor_pos_with_state(textarea_rect, state)
    }

    fn desired_height(&self, width: u16) -> u16 {
        let footer_props = self.footer_props();
        let stacked_status_line = self.stacked_status_line(&footer_props);
        let footer_props = if stacked_status_line.is_some() {
            Self::footer_props_without_stacked_status_line(&footer_props)
        } else {
            footer_props
        };
        let footer_hint_height = self
            .custom_footer_height()
            .unwrap_or_else(|| footer_height(&footer_props))
            .saturating_add(u16::from(stacked_status_line.is_some()));
        let footer_spacing = Self::footer_spacing(footer_hint_height);
        let footer_total_height = footer_hint_height + footer_spacing;
        const COLS_WITH_MARGIN: u16 = LIVE_PREFIX_COLS + 3;
        let inner_width = width.saturating_sub(COLS_WITH_MARGIN);
        let remote_images_height: u16 = self
            .remote_images_lines(inner_width)
            .len()
            .try_into()
            .unwrap_or(u16::MAX);
        let remote_images_separator = u16::from(remote_images_height > 0);
        self.textarea.desired_height(inner_width)
            + remote_images_height
            + remote_images_separator
            + 2
            + match &self.active_popup {
                ActivePopup::None => footer_total_height,
                ActivePopup::Command(c) => c.calculate_required_height(width),
                ActivePopup::File(c) => c.calculate_required_height(),
                ActivePopup::Skill(c) => c.calculate_required_height(width),
            }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_with_mask(area, buf, /*mask_char*/ None);
    }
}

impl ChatComposer {
    pub(crate) fn render_with_mask(&self, area: Rect, buf: &mut Buffer, mask_char: Option<char>) {
        let [composer_rect, remote_images_rect, textarea_rect, popup_rect] =
            self.layout_areas(area);
        match &self.active_popup {
            ActivePopup::Command(popup) => {
                popup.render_ref(popup_rect, buf);
            }
            ActivePopup::File(popup) => {
                popup.render_ref(popup_rect, buf);
            }
            ActivePopup::Skill(popup) => {
                popup.render_ref(popup_rect, buf);
            }
            ActivePopup::None => {
                let footer_props = self.footer_props();
                let stacked_status_line = self.stacked_status_line(&footer_props);
                let footer_props = if stacked_status_line.is_some() {
                    Self::footer_props_without_stacked_status_line(&footer_props)
                } else {
                    footer_props
                };
                let show_cycle_hint =
                    !footer_props.is_task_running && self.collaboration_mode_indicator.is_some();
                let show_shortcuts_hint =
                    footer_show_shortcuts_hint(footer_props.mode, footer_props.is_task_running)
                        && !self.is_in_paste_burst();
                let show_queue_hint =
                    footer_show_queue_hint(footer_props.mode, footer_props.is_task_running);
                let custom_height = self.custom_footer_height();
                let footer_hint_height = custom_height
                    .unwrap_or_else(|| footer_height(&footer_props))
                    + u16::from(stacked_status_line.is_some());
                let footer_spacing = Self::footer_spacing(footer_hint_height);
                let hint_rect = if footer_spacing > 0 && footer_hint_height > 0 {
                    let [_, hint_rect] = Layout::vertical([
                        Constraint::Length(footer_spacing),
                        Constraint::Length(footer_hint_height),
                    ])
                    .areas(popup_rect);
                    hint_rect
                } else {
                    popup_rect
                };
                let (status_line_rect, hint_rect) = if stacked_status_line.is_some() {
                    let [status_line_rect, footer_rect] =
                        Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
                            .areas(hint_rect);
                    (Some(status_line_rect), footer_rect)
                } else {
                    (None, hint_rect)
                };

                if let (Some(status_line_rect), Some(status_line)) =
                    (status_line_rect, stacked_status_line)
                {
                    let available_width = status_line_rect
                        .width
                        .saturating_sub(FOOTER_INDENT_COLS as u16)
                        as usize;
                    let mut truncated_status_line =
                        truncate_line_with_ellipsis_if_overflow(status_line, available_width);
                    let mut left_width = truncated_status_line.width() as u16;
                    let full =
                        mode_indicator_line(self.collaboration_mode_indicator, show_cycle_hint);
                    let compact = mode_indicator_line(
                        self.collaboration_mode_indicator,
                        /*show_cycle_hint*/ false,
                    );
                    let full_width = full.as_ref().map(|l| l.width() as u16).unwrap_or(0);
                    let right_line =
                        if can_show_left_with_context(status_line_rect, left_width, full_width) {
                            full
                        } else {
                            compact
                        };
                    let right_width = right_line.as_ref().map(|l| l.width() as u16).unwrap_or(0);
                    if let Some(max_left) = max_left_width_for_right(status_line_rect, right_width)
                        && left_width > max_left
                    {
                        truncated_status_line = truncate_line_with_ellipsis_if_overflow(
                            truncated_status_line,
                            max_left as usize,
                        );
                        left_width = truncated_status_line.width() as u16;
                    }

                    render_footer_line(status_line_rect, buf, truncated_status_line);
                    if can_show_left_with_context(status_line_rect, left_width, right_width)
                        && let Some(line) = &right_line
                    {
                        render_context_right(status_line_rect, buf, line);
                    }
                }

                let left_mode_indicator = self.collaboration_mode_indicator;
                let left_width = if self.footer_flash_visible() {
                    self.footer_flash
                        .as_ref()
                        .map(|flash| flash.line.width() as u16)
                        .unwrap_or(0)
                } else if let Some(items) = self.footer_hint_override.as_ref() {
                    footer_hint_items_width(items)
                } else {
                    footer_line_width(
                        &footer_props,
                        left_mode_indicator,
                        show_cycle_hint,
                        show_shortcuts_hint,
                        show_queue_hint,
                    )
                };
                let right_line = if let Some(line) = self.footer_right_badge.clone() {
                    Some(line)
                } else {
                    Some(context_window_line(
                        footer_props.context_window_percent,
                        footer_props.context_window_used_tokens,
                    ))
                };
                let right_width = right_line.as_ref().map(|l| l.width() as u16).unwrap_or(0);
                let can_show_left_and_context =
                    can_show_left_with_context(hint_rect, left_width, right_width);
                let has_override =
                    self.footer_flash_visible() || self.footer_hint_override.is_some();
                let single_line_layout = if has_override {
                    None
                } else {
                    match footer_props.mode {
                        FooterMode::ComposerEmpty | FooterMode::ComposerHasDraft => {
                            // Both of these modes render the single-line footer style (with
                            // either the shortcuts hint or the optional queue hint). We still
                            // want the single-line collapse rules so the mode label can win over
                            // the context indicator on narrow widths.
                            Some(single_line_footer_layout(
                                hint_rect,
                                right_width,
                                left_mode_indicator,
                                show_cycle_hint,
                                show_shortcuts_hint,
                                show_queue_hint,
                            ))
                        }
                        FooterMode::EscHint
                        | FooterMode::QuitShortcutReminder
                        | FooterMode::ShortcutOverlay => None,
                    }
                };
                let show_right = if matches!(
                    footer_props.mode,
                    FooterMode::EscHint
                        | FooterMode::QuitShortcutReminder
                        | FooterMode::ShortcutOverlay
                ) {
                    false
                } else {
                    single_line_layout
                        .as_ref()
                        .map(|(_, show_context)| *show_context)
                        .unwrap_or(can_show_left_and_context)
                };

                if let Some((summary_left, _)) = single_line_layout {
                    match summary_left {
                        SummaryLeft::Default => {
                            render_footer_from_props(
                                hint_rect,
                                buf,
                                &footer_props,
                                left_mode_indicator,
                                show_cycle_hint,
                                show_shortcuts_hint,
                                show_queue_hint,
                            );
                        }
                        SummaryLeft::Custom(line) => {
                            render_footer_line(hint_rect, buf, line);
                        }
                        SummaryLeft::None => {}
                    }
                } else if self.footer_flash_visible() {
                    if let Some(flash) = self.footer_flash.as_ref() {
                        flash
                            .line
                            .clone()
                            .render(inset_footer_hint_area(hint_rect), buf);
                    }
                } else if let Some(items) = self.footer_hint_override.as_ref() {
                    render_footer_hint_items(hint_rect, buf, items);
                } else {
                    render_footer_from_props(
                        hint_rect,
                        buf,
                        &footer_props,
                        self.collaboration_mode_indicator,
                        show_cycle_hint,
                        show_shortcuts_hint,
                        show_queue_hint,
                    );
                }

                if show_right && let Some(line) = &right_line {
                    render_context_right(hint_rect, buf, line);
                }
            }
        }
        let palette = self.surface_theme.visual_palette();
        let style = Style::default().fg(palette.text).bg(palette.input);
        crate::surface::render_input_surface(composer_rect, buf, self.surface_theme);
        if !remote_images_rect.is_empty() {
            Paragraph::new(self.remote_images_lines(remote_images_rect.width))
                .style(style)
                .render(remote_images_rect, buf);
        }
        if !textarea_rect.is_empty() {
            let prompt_style = if self.input_enabled {
                Style::default()
                    .fg(palette.accent_soft)
                    .bg(palette.input)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.text_inactive).bg(palette.input)
            };
            let prompt = Span::styled("❯", prompt_style);
            buf.set_span(
                textarea_rect.x - LIVE_PREFIX_COLS,
                textarea_rect.y,
                &prompt,
                LIVE_PREFIX_COLS,
            );
        }

        let mut state = self.textarea_state.borrow_mut();
        if let Some(mask_char) = mask_char {
            self.textarea
                .render_ref_masked(textarea_rect, buf, &mut state, mask_char);
        } else {
            StatefulWidgetRef::render_ref(&(&self.textarea), textarea_rect, buf, &mut state);
        }
        if self.textarea.text().is_empty() {
            let text = if self.input_enabled {
                self.placeholder_text.as_str().to_string()
            } else {
                self.input_disabled_placeholder
                    .as_deref()
                    .unwrap_or("Input disabled.")
                    .to_string()
            };
            if !textarea_rect.is_empty() && !text.is_empty() {
                let placeholder = Span::styled(
                    text,
                    Style::default().fg(palette.text_inactive).bg(palette.input),
                );
                Line::from(vec![placeholder]).render(textarea_rect.inner(Margin::new(0, 0)), buf);
            }
        }
    }
}
