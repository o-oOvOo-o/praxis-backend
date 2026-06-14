use crate::ui_language::UiLanguage;
use crate::workspace::thread_row::ThreadListRow;
use crate::workspace::thread_row::workspace_row_is_controlled;
use crate::workspace::thread_row::workspace_single_line;
use crate::workspace::thread_row::workspace_status_has_controlled_flag;
use crate::workspace::thread_row::workspace_status_has_running_flag;
use praxis_app_gateway_protocol::ThreadActiveFlag;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadControllerKind;
use praxis_app_gateway_protocol::ThreadStatus;

pub(crate) const WORKSPACE_SUBAGENT_INDENT_STEP: u16 = 3;
const WORKSPACE_SUBAGENT_INDENT_MAX: u16 = 9;

pub(crate) fn workspace_row_status_label(row: &ThreadListRow) -> String {
    let activity = match &row.status {
        ThreadStatus::Active { active_flags }
            if active_flags.contains(&ThreadActiveFlag::WaitingOnApproval)
                || active_flags.contains(&ThreadActiveFlag::WaitingOnUserInput) =>
        {
            "WAIT"
        }
        ThreadStatus::Active { .. } if workspace_status_has_running_flag(&row.status) => "RUN",
        ThreadStatus::Active { .. } if workspace_row_is_controlled(row) => "LOCK",
        ThreadStatus::Active { .. } => "RUN",
        ThreadStatus::Idle if workspace_row_is_controlled(row) => "LOCK",
        ThreadStatus::Idle => "IDLE",
        ThreadStatus::SystemError => "ERR",
        ThreadStatus::NotLoaded if workspace_row_is_controlled(row) => "LOCK",
        ThreadStatus::NotLoaded => "COLD",
    };
    let Some(control_label) = workspace_row_control_label(row) else {
        return activity.to_string();
    };
    format!("{activity} {control_label}")
}

pub(crate) fn workspace_row_subagent_marker(row: &ThreadListRow) -> Option<String> {
    if row.subagents.is_empty() {
        return None;
    }
    if row.subagents.closed > 0 {
        return Some(format!(
            "子代理 {} · 关闭 {}",
            row.subagents.open, row.subagents.closed
        ));
    }
    Some(format!("子代理 {}", row.subagents.open))
}

pub(crate) fn workspace_closed_subagents_label(count: usize, language: UiLanguage) -> String {
    match language {
        UiLanguage::En => format!("Closed subagents {count}"),
        UiLanguage::Cn => format!("已关闭子代理 {count}"),
    }
}

pub(crate) fn workspace_closed_subagents_detail(count: usize, language: UiLanguage) -> String {
    match language {
        UiLanguage::En => format!("{count} inactive subagent thread(s)"),
        UiLanguage::Cn => format!("{count} 条已关闭子代理线程"),
    }
}

pub(crate) fn workspace_row_tree_prefix(row: &ThreadListRow, expanded: bool) -> &'static str {
    if row.subagent_parent_thread_id.is_some() {
        return "└";
    }
    if row.subagents.is_empty() {
        " "
    } else if expanded {
        "▾"
    } else {
        "▸"
    }
}

fn workspace_row_tree_depth(row: &ThreadListRow) -> u16 {
    match row.subagent_depth {
        Some(depth) if depth > 0 => depth as u16,
        _ if row.subagent_parent_thread_id.is_some() => 1,
        _ => 0,
    }
}

pub(crate) fn workspace_row_tree_indent(row: &ThreadListRow) -> u16 {
    workspace_row_tree_depth(row)
        .saturating_mul(WORKSPACE_SUBAGENT_INDENT_STEP)
        .min(WORKSPACE_SUBAGENT_INDENT_MAX)
}

pub(crate) fn workspace_context_subagent_lines(
    row: Option<&ThreadListRow>,
    language: UiLanguage,
) -> Vec<String> {
    let Some(row) = row else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if !row.subagents.is_empty() {
        let closed = row.subagents.closed;
        let header = match language {
            UiLanguage::En => format!(
                "Subagents {}  open {}  running {}",
                row.subagents.total, row.subagents.open, row.subagents.running
            ),
            UiLanguage::Cn => format!(
                "子代理 {}  打开 {}  运行 {}",
                row.subagents.total, row.subagents.open, row.subagents.running
            ),
        };
        lines.push(if closed > 0 {
            match language {
                UiLanguage::En => format!("{header}  closed {closed}"),
                UiLanguage::Cn => format!("{header}  关闭 {closed}"),
            }
        } else {
            header
        });
        for label in &row.subagents.labels {
            lines.push(format!("  - {label}"));
        }
        if row.subagents.open > row.subagents.labels.len() {
            let remaining = row.subagents.open - row.subagents.labels.len();
            lines.push(match language {
                UiLanguage::En => format!("  + {remaining} more"),
                UiLanguage::Cn => format!("  + 还有 {remaining} 个"),
            });
        }
    }
    if let Some(depth) = row.subagent_depth {
        let parent = row
            .subagent_parent_thread_id
            .map(|thread_id| thread_id.to_string())
            .map(|thread_id| thread_id.chars().take(8).collect::<String>())
            .unwrap_or_else(|| "unknown".to_string());
        lines.push(match language {
            UiLanguage::En => format!("Subagent  depth {depth}  parent {parent}"),
            UiLanguage::Cn => format!("子代理  深度 {depth}  父线程 {parent}"),
        });
    }
    lines
}

fn workspace_row_control_label(row: &ThreadListRow) -> Option<String> {
    if let Some(control_state) = row.control_state.as_ref() {
        let label = match (control_state.controller.kind, control_state.controller.rank) {
            (ThreadControllerKind::External, _) => "EXT".to_string(),
            (ThreadControllerKind::Thread, Some(rank)) => format!("R{rank}"),
            (ThreadControllerKind::Thread, None) => "CTRL".to_string(),
        };
        return Some(if control_state.read_only {
            format!("{label}*")
        } else {
            label
        });
    }
    workspace_status_has_controlled_flag(&row.status).then(|| "CTRL".to_string())
}

pub(crate) fn workspace_row_control_marker(row: &ThreadListRow) -> &'static str {
    if let Some(control_state) = row.control_state.as_ref() {
        return match control_state.controller.kind {
            ThreadControllerKind::External => "E",
            ThreadControllerKind::Thread => "R",
        };
    }
    if workspace_status_has_controlled_flag(&row.status) {
        "C"
    } else {
        " "
    }
}

pub(crate) fn workspace_control_detail(
    control_state: &ThreadControlState,
    language: UiLanguage,
) -> String {
    let controller_kind = match control_state.controller.kind {
        ThreadControllerKind::Thread => language.workspace_controller_kind(true),
        ThreadControllerKind::External => language.workspace_controller_kind(false),
    };
    let rank = control_state
        .controller
        .rank
        .map(|rank| format!("R{rank} "))
        .unwrap_or_default();
    let label = control_state
        .controller
        .label
        .as_deref()
        .unwrap_or(control_state.controller.id.as_str());
    let mode = language.workspace_control_mode(control_state.read_only);
    let reason = control_state
        .reason
        .as_deref()
        .map(workspace_single_line)
        .filter(|reason| !reason.is_empty())
        .map(|reason| format!("  {reason}"))
        .unwrap_or_default();
    match language {
        UiLanguage::En => format!("{mode} by {rank}{controller_kind}:{label}{reason}"),
        UiLanguage::Cn => format!(
            "{mode}{} {rank}{controller_kind}:{label}{reason}",
            language.workspace_control_by()
        ),
    }
}
