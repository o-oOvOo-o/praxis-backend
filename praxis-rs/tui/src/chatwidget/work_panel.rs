use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::plan_tool::PlanItemArg;
use praxis_protocol::plan_tool::StepStatus;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::BorderType;
use ratatui::widgets::Borders;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use ratatui::widgets::Wrap;

use crate::style::interactive_surface_style;
use crate::text_formatting::truncate_text;

const PANEL_MIN_HEIGHT: u16 = 7;
const PANEL_MAX_HEIGHT: u16 = 18;
const PANEL_HORIZONTAL_PADDING: usize = 2;

#[derive(Clone, Debug, Default)]
pub(super) struct WorkPanelState {
    live: WorkPanelLiveState,
    plan: WorkPanelPlanState,
    selfwork: WorkPanelSelfworkState,
}

#[derive(Clone, Debug, Default)]
struct WorkPanelLiveState {
    header: Option<String>,
    details: Option<String>,
    activity: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct WorkPanelPlanState {
    explanation: Option<String>,
    items: Vec<PlanItemArg>,
}

#[derive(Clone, Debug, Default)]
struct WorkPanelSelfworkState {
    plan_path: Option<PathBuf>,
    running: bool,
    stall_count: u8,
    stall_limit: u8,
}

impl WorkPanelState {
    pub(super) fn clear_live_status(&mut self) {
        self.live = WorkPanelLiveState::default();
    }

    pub(super) fn set_live_status(
        &mut self,
        header: String,
        details: Option<String>,
        activity: Option<String>,
    ) {
        self.live.header = Some(header.trim().to_string()).filter(|header| !header.is_empty());
        self.live.details = details
            .map(|details| details.trim().to_string())
            .filter(|details| !details.is_empty());
        self.live.activity = activity
            .map(|activity| activity.trim().to_string())
            .filter(|activity| !activity.is_empty());
    }

    pub(super) fn clear_plan(&mut self) {
        self.plan = WorkPanelPlanState::default();
    }

    pub(super) fn update_plan(&mut self, update: &UpdatePlanArgs) {
        self.plan.explanation = update
            .explanation
            .as_ref()
            .map(|explanation| explanation.trim().to_string())
            .filter(|explanation| !explanation.is_empty());
        self.plan.items = update.plan.clone();
    }

    pub(super) fn set_selfwork(
        &mut self,
        plan_path: Option<PathBuf>,
        running: bool,
        stall_count: u8,
        stall_limit: u8,
    ) {
        self.selfwork = WorkPanelSelfworkState {
            plan_path,
            running,
            stall_count,
            stall_limit,
        };
    }

    pub(super) fn has_content(&self) -> bool {
        self.live.header.is_some()
            || self.live.details.is_some()
            || self.live.activity.is_some()
            || self.selfwork.plan_path.is_some()
            || self.plan.explanation.is_some()
            || !self.plan.items.is_empty()
    }

    pub(super) fn desired_height(&self, width: u16) -> u16 {
        if !self.has_content() || width < 8 {
            return 0;
        }

        let content_width = usize::from(width).saturating_sub(PANEL_HORIZONTAL_PADDING);
        let max_content_rows = usize::from(PANEL_MAX_HEIGHT.saturating_sub(2));
        let rows = self.lines(content_width, max_content_rows).len();
        let desired = u16::try_from(rows.saturating_add(2)).unwrap_or(PANEL_MAX_HEIGHT);
        desired.clamp(PANEL_MIN_HEIGHT, PANEL_MAX_HEIGHT)
    }

    pub(super) fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || !self.has_content() {
            return;
        }

        let border_style = Style::default().fg(Color::DarkGray);
        let title_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let block = Block::default()
            .title(Span::styled(" Work ", title_style))
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(border_style)
            .style(interactive_surface_style());
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.is_empty() {
            return;
        }

        let max_rows = usize::from(inner.height);
        let content_width = usize::from(inner.width);
        let lines = self.lines(content_width, max_rows);
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }

    fn lines(&self, content_width: usize, max_rows: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(max_rows.min(12).max(1));
        self.push_live_lines(content_width, max_rows, &mut lines);
        self.push_selfwork_lines(content_width, max_rows, &mut lines);
        self.push_plan_lines(content_width, max_rows, &mut lines);
        lines
    }

    fn push_live_lines(
        &self,
        content_width: usize,
        max_rows: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        let has_live = self.live.header.is_some()
            || self.live.details.is_some()
            || self.live.activity.is_some();
        if !has_live || lines.len() >= max_rows {
            return;
        }

        if let Some(header) = self.live.header.as_deref()
            && lines.len() < max_rows
        {
            lines.push(Line::from(vec![
                Span::styled("Now ", label_style()),
                Span::styled(
                    truncate_text(header, content_width.saturating_sub(4)),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        if let Some(activity) = self.live.activity.as_deref()
            && lines.len() < max_rows
        {
            lines.push(Line::from(vec![
                Span::styled("Doing ", label_style()),
                Span::styled(
                    truncate_text(activity, content_width.saturating_sub(6)),
                    strong_style(),
                ),
            ]));
        }

        if let Some(details) = self.live.details.as_deref() {
            for detail in details
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .take(2)
            {
                if lines.len() >= max_rows {
                    break;
                }
                lines.push(Line::from(vec![
                    Span::styled("Info ", label_style()),
                    Span::styled(
                        truncate_text(detail, content_width.saturating_sub(5)),
                        muted_style(),
                    ),
                ]));
            }
        }

        push_blank_if_room(lines, max_rows);
    }

    fn push_selfwork_lines(
        &self,
        content_width: usize,
        max_rows: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        let Some(path) = self.selfwork.plan_path.as_deref() else {
            return;
        };
        if lines.len() >= max_rows {
            return;
        }

        lines.push(Line::from(vec![
            Span::styled("Goal ", label_style()),
            Span::styled(
                display_plan_path(path, content_width.saturating_sub(5)),
                strong_style(),
            ),
        ]));

        if lines.len() < max_rows {
            let state = if self.selfwork.running {
                (
                    "running",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    "armed",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            };
            let mut spans = vec![
                Span::styled("Loop ", label_style()),
                Span::styled(state.0.to_string(), state.1),
            ];
            if self.selfwork.stall_count > 0 {
                spans.push(Span::styled(
                    format!(
                        "  unchanged {}/{}",
                        self.selfwork.stall_count, self.selfwork.stall_limit
                    ),
                    muted_style(),
                ));
            }
            lines.push(Line::from(spans));
        }

        push_blank_if_room(lines, max_rows);
    }

    fn push_plan_lines(
        &self,
        content_width: usize,
        max_rows: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        if self.plan.explanation.is_none() && self.plan.items.is_empty() {
            return;
        }

        if let Some(explanation) = self.plan.explanation.as_deref()
            && lines.len() < max_rows
        {
            lines.push(Line::from(vec![
                Span::styled("Plan ", label_style()),
                Span::styled(
                    truncate_text(explanation, content_width.saturating_sub(5)),
                    strong_style(),
                ),
            ]));
        }

        if self.plan.items.is_empty() || lines.len() >= max_rows {
            return;
        }

        let completed = self
            .plan
            .items
            .iter()
            .filter(|item| matches!(&item.status, StepStatus::Completed))
            .count();
        if lines.len() < max_rows {
            lines.push(Line::from(vec![
                Span::styled("Tasks ", label_style()),
                Span::styled(
                    format!("{completed}/{}", self.plan.items.len()),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        let remaining_rows = max_rows.saturating_sub(lines.len());
        let visible_items = remaining_rows.min(self.plan.items.len());
        for item in self.plan.items.iter().take(visible_items) {
            lines.push(plan_item_line(item, content_width));
        }
        if visible_items < self.plan.items.len() && lines.len() < max_rows {
            lines.push(Line::from(Span::styled(
                format!(
                    "... {} more",
                    self.plan.items.len().saturating_sub(visible_items)
                ),
                muted_style(),
            )));
        }
    }
}

fn plan_item_line(item: &PlanItemArg, content_width: usize) -> Line<'static> {
    let (marker, style) = match &item.status {
        StepStatus::Completed => (
            "[x] ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::CROSSED_OUT),
        ),
        StepStatus::InProgress => (
            "[~] ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        StepStatus::Pending => ("[ ] ", muted_style()),
    };
    let text_width = content_width.saturating_sub(marker.len());
    Line::from(vec![
        Span::styled(marker, style),
        Span::styled(truncate_text(item.step.as_str(), text_width), style),
    ])
}

fn display_plan_path(path: &Path, width: usize) -> String {
    let display = path.display().to_string();
    truncate_text(&display, width)
}

fn push_blank_if_room(lines: &mut Vec<Line<'static>>, max_rows: usize) {
    if lines.len() < max_rows {
        lines.push(Line::from(""));
    }
}

fn label_style() -> Style {
    muted_style().add_modifier(Modifier::BOLD)
}

fn strong_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn muted_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        let mut text = String::new();
        for span in &line.spans {
            text.push_str(span.content.as_ref());
        }
        text
    }

    fn line_texts(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(line_text).collect()
    }

    #[test]
    fn empty_panel_has_no_render_height() {
        let panel = WorkPanelState::default();
        assert!(!panel.has_content());
        assert_eq!(panel.desired_height(36), 0);
    }

    #[test]
    fn plan_update_projects_explanation_and_task_counts() {
        let mut panel = WorkPanelState::default();
        panel.update_plan(&UpdatePlanArgs {
            explanation: Some("  Ship the TUI surface  ".to_string()),
            plan: vec![
                PlanItemArg {
                    step: "Extract work panel".to_string(),
                    status: StepStatus::Completed,
                },
                PlanItemArg {
                    step: "Wire chat layout".to_string(),
                    status: StepStatus::InProgress,
                },
                PlanItemArg {
                    step: "Polish chrome".to_string(),
                    status: StepStatus::Pending,
                },
            ],
        });

        let texts = line_texts(&panel.lines(40, 12));
        assert!(texts.iter().any(|line| line == "Plan Ship the TUI surface"));
        assert!(texts.iter().any(|line| line == "Tasks 1/3"));
        assert!(texts.iter().any(|line| line == "[x] Extract work panel"));
        assert!(texts.iter().any(|line| line == "[~] Wire chat layout"));
        assert!(texts.iter().any(|line| line == "[ ] Polish chrome"));
    }

    #[test]
    fn selfwork_and_plan_share_the_panel_without_dropping_state() {
        let mut panel = WorkPanelState::default();
        panel.set_selfwork(
            Some(PathBuf::from("plans/praxis.md")),
            /*running*/ true,
            /*stall_count*/ 2,
            /*stall_limit*/ 3,
        );
        panel.update_plan(&UpdatePlanArgs {
            explanation: Some("Keep moving".to_string()),
            plan: vec![PlanItemArg {
                step: "Run the next item".to_string(),
                status: StepStatus::InProgress,
            }],
        });

        let texts = line_texts(&panel.lines(48, 12));
        assert!(texts.iter().any(|line| line.starts_with("Goal ")));
        assert!(texts.iter().any(|line| line.contains("running")));
        assert!(texts.iter().any(|line| line.contains("unchanged 2/3")));
        assert!(texts.iter().any(|line| line == "Plan Keep moving"));
        assert!(texts.iter().any(|line| line == "[~] Run the next item"));
    }

    #[test]
    fn lines_never_exceed_requested_rows() {
        let mut panel = WorkPanelState::default();
        panel.update_plan(&UpdatePlanArgs {
            explanation: Some("Long plan".to_string()),
            plan: (0..20)
                .map(|index| PlanItemArg {
                    step: format!("Step {index}"),
                    status: StepStatus::Pending,
                })
                .collect(),
        });

        let lines = panel.lines(30, 5);
        assert!(lines.len() <= 5);
        assert_eq!(panel.desired_height(30), PANEL_MAX_HEIGHT);
    }

    #[test]
    fn clear_plan_preserves_selfwork_goal() {
        let mut panel = WorkPanelState::default();
        panel.set_selfwork(
            Some(PathBuf::from("plan.md")),
            /*running*/ false,
            /*stall_count*/ 0,
            /*stall_limit*/ 3,
        );
        panel.update_plan(&UpdatePlanArgs {
            explanation: Some("Temporary".to_string()),
            plan: vec![PlanItemArg {
                step: "Temporary step".to_string(),
                status: StepStatus::Pending,
            }],
        });

        panel.clear_plan();

        let texts = line_texts(&panel.lines(36, 8));
        assert!(texts.iter().any(|line| line.starts_with("Goal ")));
        assert!(texts.iter().all(|line| !line.starts_with("Plan ")));
        assert!(texts.iter().all(|line| !line.starts_with("Tasks ")));
    }
}
