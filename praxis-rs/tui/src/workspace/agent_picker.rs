use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use praxis_protocol::ThreadId;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

#[derive(Debug, Clone)]
pub(crate) enum AgentPickerEffect {
    None,
    Close,
    Select(ThreadId),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AgentPickerState {
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) view_rows: usize,
    pub(crate) rows: Vec<AgentPickerRow>,
    pub(crate) subtitle: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentPickerRow {
    pub(crate) thread_id: ThreadId,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) is_current: bool,
    pub(crate) is_closed: bool,
    pub(crate) search_value: String,
}

impl AgentPickerState {
    pub(crate) fn new(
        rows: Vec<AgentPickerRow>,
        initial_selected_idx: Option<usize>,
        subtitle: String,
    ) -> Self {
        let mut state = Self {
            query: String::new(),
            selected: initial_selected_idx.unwrap_or_default(),
            scroll: 0,
            view_rows: 0,
            rows,
            subtitle,
        };
        state.clamp_selection();
        state
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> AgentPickerEffect {
        match key.code {
            KeyCode::Esc => AgentPickerEffect::Close,
            KeyCode::Enter => self.activate_selected(),
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.ensure_selected_visible();
                AgentPickerEffect::None
            }
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.item_count().saturating_sub(1));
                self.ensure_selected_visible();
                AgentPickerEffect::None
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(self.view_rows.max(1));
                self.ensure_selected_visible();
                AgentPickerEffect::None
            }
            KeyCode::PageDown => {
                self.selected = self
                    .selected
                    .saturating_add(self.view_rows.max(1))
                    .min(self.item_count().saturating_sub(1));
                self.ensure_selected_visible();
                AgentPickerEffect::None
            }
            KeyCode::Home => {
                self.selected = 0;
                self.ensure_selected_visible();
                AgentPickerEffect::None
            }
            KeyCode::End => {
                self.selected = self.item_count().saturating_sub(1);
                self.ensure_selected_visible();
                AgentPickerEffect::None
            }
            KeyCode::Backspace => {
                if self.query.pop().is_some() {
                    self.selected = 0;
                    self.scroll = 0;
                }
                AgentPickerEffect::None
            }
            KeyCode::Delete => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.selected = 0;
                    self.scroll = 0;
                }
                AgentPickerEffect::None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.selected = 0;
                    self.scroll = 0;
                }
                AgentPickerEffect::None
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.query.push(c);
                self.selected = 0;
                self.scroll = 0;
                AgentPickerEffect::None
            }
            _ => AgentPickerEffect::None,
        }
    }

    pub(crate) fn set_view_rows(&mut self, rows: usize) {
        self.view_rows = rows;
        self.ensure_selected_visible();
    }

    pub(crate) fn scroll_by(&mut self, delta: isize) -> bool {
        let previous = self.scroll;
        let amount = delta.unsigned_abs();
        if delta < 0 {
            self.scroll = self.scroll.saturating_sub(amount);
        } else {
            let max_scroll = self.item_count().saturating_sub(self.view_rows.max(1));
            self.scroll = self.scroll.saturating_add(amount).min(max_scroll);
        }
        self.scroll != previous
    }

    pub(crate) fn selected_index_at_row(&self, relative_row: u16) -> Option<usize> {
        let index = self.scroll.saturating_add(usize::from(relative_row / 3));
        (index < self.item_count()).then_some(index)
    }

    pub(crate) fn set_selected(&mut self, selected: usize) {
        self.selected = selected.min(self.item_count().saturating_sub(1));
        self.ensure_selected_visible();
    }

    pub(crate) fn activate_selected(&self) -> AgentPickerEffect {
        let Some(row) = self.filtered_rows().get(self.selected).copied() else {
            return AgentPickerEffect::None;
        };
        AgentPickerEffect::Select(row.thread_id)
    }

    fn item_count(&self) -> usize {
        self.filtered_rows().len()
    }

    fn filtered_rows(&self) -> Vec<&AgentPickerRow> {
        let query = self.query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return self.rows.iter().collect();
        }
        self.rows
            .iter()
            .filter(|row| row.search_value.to_ascii_lowercase().contains(&query))
            .collect()
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.item_count().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn ensure_selected_visible(&mut self) {
        let view_rows = self.view_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(view_rows) {
            self.scroll = self.selected.saturating_add(1).saturating_sub(view_rows);
        }
        self.scroll = self.scroll.min(self.item_count().saturating_sub(view_rows));
    }
}

pub(super) fn render_agent_picker(area: Rect, buf: &mut Buffer, state: &AgentPickerState) {
    if area.is_empty() {
        return;
    }
    let [header, search, list] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .areas(area);

    Block::default()
        .style(Style::default().bg(Color::Rgb(18, 20, 20)).fg(Color::Gray))
        .render(area, buf);

    Paragraph::new(Line::from(vec![
        Span::styled(
            " Subagents ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(state.subtitle.clone(), Style::default().fg(Color::DarkGray)),
        Span::styled("   Esc back", Style::default().fg(Color::DarkGray)),
    ]))
    .render(header, buf);

    let query = if state.query.is_empty() {
        "Search agents".to_string()
    } else {
        state.query.clone()
    };
    Paragraph::new(Line::from(vec![
        Span::styled("  / ", Style::default().fg(Color::Rgb(138, 190, 150))),
        Span::styled(
            query,
            Style::default().fg(if state.query.is_empty() {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
    ]))
    .render(search, buf);

    let filtered = state.filtered_rows();
    let row_height = 3usize;
    let visible_rows = (list.height as usize / row_height).max(1);
    let start = state.scroll.min(filtered.len());
    let end = filtered.len().min(start.saturating_add(visible_rows));
    for index in start..end {
        let y = list.y.saturating_add(((index - start) * row_height) as u16);
        let row_area = Rect::new(list.x, y, list.width, row_height as u16);
        render_agent_row(row_area, buf, filtered[index], index == state.selected);
    }

    if filtered.is_empty() {
        Paragraph::new(if state.rows.is_empty() {
            "No agents available yet"
        } else {
            "No agents match the search"
        })
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
        .render(list, buf);
    }
}

fn render_agent_row(area: Rect, buf: &mut Buffer, row: &AgentPickerRow, selected: bool) {
    if area.is_empty() {
        return;
    }
    let bg = if selected {
        Color::Rgb(42, 55, 45)
    } else {
        Color::Rgb(18, 20, 20)
    };
    let status = if row.is_closed { "closed" } else { "open" };
    let status_color = if row.is_closed {
        Color::DarkGray
    } else {
        Color::Rgb(138, 190, 150)
    };
    buf.set_style(area, Style::default().bg(bg));
    let lines = vec![
        Line::from(vec![
            Span::styled(
                if selected { "| " } else { "  " },
                Style::default().fg(Color::Rgb(138, 190, 150)).bg(bg),
            ),
            Span::styled(
                truncate(&row.name, area.width.saturating_sub(16) as usize),
                Style::default()
                    .fg(Color::White)
                    .bg(bg)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                if row.is_current { "  current" } else { "" },
                Style::default().fg(Color::Rgb(138, 190, 150)).bg(bg),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(status, Style::default().fg(status_color).bg(bg)),
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(
                truncate(&row.description, area.width.saturating_sub(12) as usize),
                Style::default().fg(Color::Gray).bg(bg),
            ),
        ]),
        Line::from(vec![Span::styled(
            format!("  {}", row.thread_id),
            Style::default().fg(Color::DarkGray).bg(bg),
        )]),
    ];
    Paragraph::new(lines).render(area, buf);
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count.saturating_add(1) >= width {
            out.push_str("...");
            return out;
        }
        out.push(ch);
        count = count.saturating_add(1);
    }
    out
}
