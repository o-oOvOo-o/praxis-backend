use praxis_app_gateway_protocol::ThreadGoalStatus as AppGatewayThreadGoalStatus;
use praxis_protocol::protocol::HookEventName;

use crate::text_formatting::truncate_text;

pub(super) const DEFAULT_COMPOSER_PLACEHOLDER: &str =
    "Message Praxis  / for commands  @ for context";

pub(super) fn extract_first_bold(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'*' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < bytes.len() {
                if bytes[j] == b'*' && bytes[j + 1] == b'*' {
                    let trimmed = s[start..j].trim();
                    return (!trimmed.is_empty()).then(|| trimmed.to_string());
                }
                j += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

pub(super) fn reasoning_status_preview(text: &str, max_lines: usize) -> Option<String> {
    let max_lines = max_lines.max(1);
    let mut lines = text
        .lines()
        .rev()
        .filter_map(normalize_reasoning_status_line)
        .take(max_lines)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join("\n"))
}

pub(super) fn format_goal_elapsed(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        return "<1m".to_string();
    }
    let minutes = (seconds + 59) / 60;
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    match (hours, remaining_minutes) {
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h{minutes}m"),
    }
}

pub(super) fn app_gateway_goal_status_label(status: AppGatewayThreadGoalStatus) -> &'static str {
    match status {
        AppGatewayThreadGoalStatus::Active => "active",
        AppGatewayThreadGoalStatus::Paused => "paused",
        AppGatewayThreadGoalStatus::Blocked => "blocked",
        AppGatewayThreadGoalStatus::UsageLimited => "usage limited",
        AppGatewayThreadGoalStatus::BudgetLimited => "budget limited",
        AppGatewayThreadGoalStatus::Complete => "complete",
    }
}

pub(super) fn edited_goal_status(status: AppGatewayThreadGoalStatus) -> AppGatewayThreadGoalStatus {
    match status {
        AppGatewayThreadGoalStatus::Active => AppGatewayThreadGoalStatus::Active,
        AppGatewayThreadGoalStatus::Paused
        | AppGatewayThreadGoalStatus::Blocked
        | AppGatewayThreadGoalStatus::UsageLimited => status,
        AppGatewayThreadGoalStatus::BudgetLimited | AppGatewayThreadGoalStatus::Complete => {
            AppGatewayThreadGoalStatus::Active
        }
    }
}

pub(super) fn hook_event_label(event_name: HookEventName) -> &'static str {
    match event_name {
        HookEventName::PreToolUse => "PreToolUse",
        HookEventName::PostToolUse => "PostToolUse",
        HookEventName::SessionStart => "SessionStart",
        HookEventName::UserPromptSubmit => "UserPromptSubmit",
        HookEventName::Stop => "Stop",
    }
}

fn normalize_reasoning_status_line(line: &str) -> Option<String> {
    let normalized = line.replace("**", "");
    let normalized = normalized.trim();
    if normalized.is_empty()
        || normalized
            .chars()
            .all(|ch| matches!(ch, '*' | '-' | '_' | '#' | '=' | '`'))
    {
        None
    } else {
        Some(truncate_text(normalized, 220))
    }
}
