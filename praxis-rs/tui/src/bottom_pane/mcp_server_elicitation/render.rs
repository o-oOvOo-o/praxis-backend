use super::*;

impl McpServerElicitationOverlay {
    fn render_prompt(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let answered = self.is_current_field_answered();
        for (offset, line) in self.wrapped_prompt_lines(area.width).iter().enumerate() {
            let y = area.y.saturating_add(offset as u16);
            if y >= area.y + area.height {
                break;
            }
            let line = if answered {
                Line::from(line.clone())
            } else {
                Line::from(line.clone()).cyan()
            };
            Paragraph::new(line).render(
                Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
                buf,
            );
        }
    }

    fn render_input(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        if self.current_field_is_select() {
            let rows = self.option_rows();
            let mut state = self
                .current_answer()
                .map(|answer| answer.selection)
                .unwrap_or_default();
            if state.selected_idx.is_none() && !rows.is_empty() {
                state.selected_idx = Some(0);
            }
            state.ensure_visible(rows.len(), area.height as usize);
            render_rows(area, buf, &rows, &state, rows.len().max(1), "No options");
            return;
        }
        if self.current_field_is_secret() {
            self.composer.render_with_mask(area, buf, Some('*'));
        } else {
            self.composer.render(area, buf);
        }
    }

    fn render_footer(&self, area: Rect, input_area_height: u16, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let options_hidden = self.current_field_is_select()
            && input_area_height > 0
            && self.options_required_height(area.width) > input_area_height;
        let option_tip = if options_hidden {
            let selected = self.selected_option_index().unwrap_or(0).saturating_add(1);
            let total = self.options_len();
            Some(FooterTip::new(format!("option {selected}/{total}")))
        } else {
            None
        };
        let mut tip_lines = self.footer_tip_lines(area.width);
        if let Some(prefix) = option_tip {
            let mut tips = vec![prefix];
            if let Some(first_line) = tip_lines.first_mut() {
                let mut first = Vec::new();
                std::mem::swap(first_line, &mut first);
                tips.extend(first);
                *first_line = tips;
            } else {
                tip_lines.push(tips);
            }
        }
        for (row_idx, tips) in tip_lines.into_iter().take(area.height as usize).enumerate() {
            let mut spans = Vec::new();
            for (tip_idx, tip) in tips.into_iter().enumerate() {
                if tip_idx > 0 {
                    spans.push(FOOTER_SEPARATOR.into());
                }
                if tip.highlight {
                    spans.push(tip.text.cyan().bold().not_dim());
                } else {
                    spans.push(tip.text.into());
                }
            }
            let line = Line::from(spans).dim();
            Paragraph::new(line).render(
                Rect {
                    x: area.x,
                    y: area.y.saturating_add(row_idx as u16),
                    width: area.width,
                    height: 1,
                },
                buf,
            );
        }
    }
}

impl Renderable for McpServerElicitationOverlay {
    fn desired_height(&self, width: u16) -> u16 {
        let outer = Rect::new(0, 0, width, u16::MAX);
        let inner = menu_surface_inset(outer);
        let inner_width = inner.width.max(1);
        let height = 1u16
            .saturating_add(self.wrapped_prompt_lines(inner_width).len() as u16)
            .saturating_add(self.input_height(inner_width))
            .saturating_add(self.footer_tip_lines(inner_width).len() as u16)
            .saturating_add(menu_surface_padding_height());
        height.max(MIN_OVERLAY_HEIGHT)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let content_area = render_menu_surface(area, buf);
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }
        let prompt_lines = self.wrapped_prompt_lines(content_area.width);
        let footer_lines = self.footer_tip_lines(content_area.width);
        let mut remaining = content_area.height;

        let progress_height = u16::from(remaining > 0);
        remaining = remaining.saturating_sub(progress_height);

        let footer_height = (footer_lines.len() as u16).min(remaining.saturating_sub(1));
        remaining = remaining.saturating_sub(footer_height);

        let min_input_height = if self.current_field_is_select() {
            u16::from(remaining > 0)
        } else {
            MIN_COMPOSER_HEIGHT.min(remaining)
        };
        let mut input_height = min_input_height;
        remaining = remaining.saturating_sub(input_height);

        let prompt_height = (prompt_lines.len() as u16).min(remaining);
        remaining = remaining.saturating_sub(prompt_height);
        input_height = input_height.saturating_add(remaining);

        let progress_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: content_area.width,
            height: progress_height,
        };
        let prompt_area = Rect {
            x: content_area.x,
            y: progress_area.y.saturating_add(progress_area.height),
            width: content_area.width,
            height: prompt_height,
        };
        let input_area = Rect {
            x: content_area.x,
            y: prompt_area.y.saturating_add(prompt_area.height),
            width: content_area.width,
            height: input_height,
        };
        let footer_area = Rect {
            x: content_area.x,
            y: input_area.y.saturating_add(input_area.height),
            width: content_area.width,
            height: footer_height,
        };

        let unanswered = self.required_unanswered_count();
        let progress_line = if self.field_count() > 0 {
            let idx = self.current_index() + 1;
            let total = self.field_count();
            let base = format!("Field {idx}/{total}");
            if unanswered > 0 {
                Line::from(format!("{base} ({unanswered} required unanswered)").dim())
            } else {
                Line::from(base.dim())
            }
        } else {
            Line::from("No fields".dim())
        };
        Paragraph::new(progress_line).render(progress_area, buf);
        self.render_prompt(prompt_area, buf);
        self.render_input(input_area, buf);
        self.render_footer(footer_area, input_area.height, buf);
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        if self.current_field_is_select() {
            return None;
        }
        let content_area = menu_surface_inset(area);
        if content_area.width == 0 || content_area.height == 0 {
            return None;
        }
        let prompt_lines = self.wrapped_prompt_lines(content_area.width);
        let footer_lines = self.footer_tip_lines(content_area.width);
        let mut remaining = content_area.height;
        remaining = remaining.saturating_sub(u16::from(remaining > 0));
        let footer_height = (footer_lines.len() as u16).min(remaining.saturating_sub(1));
        remaining = remaining.saturating_sub(footer_height);
        let min_input_height = MIN_COMPOSER_HEIGHT.min(remaining);
        let mut input_height = min_input_height;
        remaining = remaining.saturating_sub(input_height);
        let prompt_height = (prompt_lines.len() as u16).min(remaining);
        remaining = remaining.saturating_sub(prompt_height);
        input_height = input_height.saturating_add(remaining);
        let input_area = Rect {
            x: content_area.x,
            y: content_area
                .y
                .saturating_add(1)
                .saturating_add(prompt_height),
            width: content_area.width,
            height: input_height,
        };
        self.composer.cursor_pos(input_area)
    }
}

pub(super) fn wrap_footer_tips(width: u16, tips: Vec<FooterTip>) -> Vec<Vec<FooterTip>> {
    let max_width = width.max(1) as usize;
    let separator_width = UnicodeWidthStr::width(FOOTER_SEPARATOR);
    if tips.is_empty() {
        return vec![Vec::new()];
    }

    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut used = 0usize;

    for tip in tips {
        let tip_width = UnicodeWidthStr::width(tip.text.as_str()).min(max_width);
        let extra = if current.is_empty() {
            tip_width
        } else {
            separator_width.saturating_add(tip_width)
        };
        if !current.is_empty() && used.saturating_add(extra) > max_width {
            lines.push(current);
            current = Vec::new();
            used = 0;
        }
        if current.is_empty() {
            used = tip_width;
        } else {
            used = used
                .saturating_add(separator_width)
                .saturating_add(tip_width);
        }
        current.push(tip);
    }

    if current.is_empty() {
        lines.push(Vec::new());
    } else {
        lines.push(current);
    }
    lines
}
