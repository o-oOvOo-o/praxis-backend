use crate::status::format_tokens_compact;
use crate::workspace::WorkspaceTheme;
use crate::workspace::WorkspaceThemeKind;
use crate::workspace::workspace_single_line;
use praxis_app_gateway_protocol::ThreadGoalStatus;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::visual::InteractiveState;
use ratatui::visual::render_accent_bar;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

pub(super) fn workspace_row_style(
    theme: WorkspaceTheme,
    is_active: bool,
    is_selected: bool,
    is_hovered: bool,
    is_controlled: bool,
) -> Style {
    let style = theme.visual_palette().row_style(InteractiveState::new(
        is_active,
        is_selected,
        is_hovered,
        is_controlled,
    ));
    if matches!(theme.kind, WorkspaceThemeKind::Classic) && (is_selected || is_active) {
        style.fg(theme.panel_bg).add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

pub(super) fn workspace_row_accent(
    theme: WorkspaceTheme,
    is_active: bool,
    is_selected: bool,
    is_hovered: bool,
    is_controlled: bool,
) -> Option<Color> {
    theme.visual_palette().row_accent(InteractiveState::new(
        is_active,
        is_selected,
        is_hovered,
        is_controlled,
    ))
}

pub(super) fn render_workspace_row_accent(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    style: Style,
    color: Option<Color>,
) {
    render_accent_bar(buf, area, style, color);
}

pub(super) fn workspace_status_style(
    theme: WorkspaceTheme,
    color: Color,
    controlled: bool,
) -> Style {
    theme.visual_palette().status_style(
        InteractiveState::new(false, false, false, controlled),
        color,
    )
}

pub(super) fn workspace_search_term(query: &str) -> Option<String> {
    let query = query.trim();
    (!query.is_empty()).then(|| query.to_string())
}

pub(super) fn previous_char_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(super) fn next_char_boundary(value: &str, cursor: usize) -> usize {
    value[cursor..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| cursor + offset)
        .unwrap_or(value.len())
}

pub(super) fn workspace_cache_label(
    info: Option<&TokenUsageInfo>,
    expanded: bool,
) -> Option<String> {
    let info = info?;
    let mut parts = Vec::new();
    if let Some(part) = workspace_cache_segment("L", &info.last_token_usage, expanded) {
        parts.push(part);
    }
    if let Some(part) = workspace_cache_segment("T", &info.total_token_usage, expanded) {
        parts.push(part);
    }
    (!parts.is_empty()).then(|| format!("cache {}", parts.join("/")))
}

fn workspace_cache_segment(prefix: &str, usage: &TokenUsage, expanded: bool) -> Option<String> {
    let percent = usage.cache_hit_percent()?;
    if !expanded {
        return Some(format!("{prefix}{percent}%"));
    }
    let reported = usage.cache_reported_input();
    let cached = usage.cached_input().min(reported);
    Some(format!(
        "{prefix}{percent}%({}/{})",
        format_tokens_compact(cached),
        format_tokens_compact(reported)
    ))
}

pub(super) fn workspace_truncate(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let text = workspace_single_line(text);
    if UnicodeWidthStr::width(text.as_str()) <= max_chars {
        return text;
    }
    if max_chars == 1 {
        return "~".to_string();
    }

    let mut out = String::new();
    let mut used = 1; // reserve room for the ASCII truncation marker
    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + width > max_chars {
            break;
        }
        used += width;
        out.push(ch);
    }
    out.push('~');
    out
}

pub(super) fn truncate_for_goal_notice(text: &str) -> String {
    let normalized = workspace_single_line(text);
    if normalized.chars().count() <= 96 {
        return normalized;
    }
    let mut out = normalized.chars().take(95).collect::<String>();
    out.push('~');
    out
}

pub(super) fn goal_status_label(status: ThreadGoalStatus) -> &'static str {
    match status {
        ThreadGoalStatus::Active => "active",
        ThreadGoalStatus::Paused => "paused",
        ThreadGoalStatus::Blocked => "blocked",
        ThreadGoalStatus::UsageLimited => "usage limited",
        ThreadGoalStatus::BudgetLimited => "budget limited",
        ThreadGoalStatus::Complete => "complete",
    }
}
