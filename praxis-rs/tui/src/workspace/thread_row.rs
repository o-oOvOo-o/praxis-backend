use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::multi_agents::subagent_display_name;
use praxis_app_gateway_protocol::SessionSource as AppGatewaySessionSource;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadActiveFlag;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::TokenUsageInfo;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct ThreadListRow {
    pub(crate) thread_id: ThreadId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) name: String,
    pub(crate) preview: String,
    pub(crate) cwd: PathBuf,
    pub(crate) status: ThreadStatus,
    pub(crate) control_state: Option<ThreadControlState>,
    pub(crate) source: String,
    pub(crate) source_kind: AppGatewaySessionSource,
    pub(crate) agent_base_name: Option<String>,
    pub(crate) agent_title: Option<String>,
    pub(crate) agent_display_name: Option<String>,
    pub(crate) subagent_parent_thread_id: Option<ThreadId>,
    pub(crate) subagent_depth: Option<i32>,
    pub(crate) subagents: WorkspaceSubagentSummary,
    pub(crate) updated_at: i64,
    pub(crate) token_usage: Option<TokenUsageInfo>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceSubagentSummary {
    pub(crate) total: usize,
    pub(crate) open: usize,
    pub(crate) closed: usize,
    pub(crate) running: usize,
    pub(crate) labels: Vec<String>,
}

impl WorkspaceSubagentSummary {
    pub(crate) fn is_empty(&self) -> bool {
        self.total == 0
    }
}

#[derive(Debug, Clone)]
struct WorkspaceSubagentChildSummary {
    parent_thread_id: ThreadId,
    label: String,
    is_open: bool,
    is_running: bool,
}

impl ThreadListRow {
    pub(crate) fn from_thread(thread: Thread) -> Option<Self> {
        let thread_id = ThreadId::from_string(&thread.id).ok()?;
        let (subagent_parent_thread_id, subagent_depth) =
            workspace_subagent_parent(&thread.source).unwrap_or((None, None));
        let agent_display_name = thread
            .agent_display_name
            .clone()
            .or_else(|| workspace_subagent_display_name(&thread.source));
        let agent_base_name = thread
            .agent_base_name
            .clone()
            .or_else(|| workspace_subagent_base_name(&thread.source));
        let agent_title = thread
            .agent_title
            .clone()
            .or_else(|| workspace_subagent_title(&thread.source));
        let mut name = thread
            .name
            .clone()
            .or_else(|| agent_display_name.clone())
            .or_else(|| thread.summary.clone())
            .or_else(|| {
                let preview = thread.preview.trim();
                (!preview.is_empty()).then(|| preview.to_string())
            })
            .unwrap_or_else(|| workspace_fallback_thread_name(&thread.id));
        name = workspace_single_line(&name);
        let preview = if let Some(title) = agent_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
        {
            title.to_string()
        } else if thread.preview.trim().is_empty() {
            thread
                .agent_role
                .clone()
                .unwrap_or_else(|| thread.model_provider.clone())
        } else {
            thread.preview.clone()
        };

        Some(Self {
            thread_id,
            path: thread.path,
            name,
            preview,
            cwd: thread.cwd,
            status: thread.status,
            control_state: thread.control_state,
            source: workspace_source_label(&thread.source),
            source_kind: thread.source,
            agent_base_name,
            agent_title,
            agent_display_name,
            subagent_parent_thread_id,
            subagent_depth,
            subagents: WorkspaceSubagentSummary::default(),
            updated_at: thread.updated_at,
            token_usage: thread.token_usage.map(token_usage_info_from_app_gateway),
        })
    }
}

fn workspace_subagent_display_name(source: &AppGatewaySessionSource) -> Option<String> {
    match source {
        AppGatewaySessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            agent_display_name,
            ..
        }) => agent_display_name.clone(),
        _ => None,
    }
}

fn workspace_subagent_base_name(source: &AppGatewaySessionSource) -> Option<String> {
    match source {
        AppGatewaySessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            agent_base_name, ..
        }) => agent_base_name.clone(),
        _ => None,
    }
}

fn workspace_subagent_title(source: &AppGatewaySessionSource) -> Option<String> {
    match source {
        AppGatewaySessionSource::SubAgent(SubAgentSource::ThreadSpawn { agent_title, .. }) => {
            agent_title.clone()
        }
        _ => None,
    }
}

fn workspace_subagent_parent(
    source: &AppGatewaySessionSource,
) -> Option<(Option<ThreadId>, Option<i32>)> {
    match source {
        AppGatewaySessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            ..
        }) => Some((Some(*parent_thread_id), Some(*depth))),
        AppGatewaySessionSource::SubAgent(_) => Some((None, None)),
        _ => None,
    }
}

fn workspace_source_label(source: &AppGatewaySessionSource) -> String {
    match source {
        AppGatewaySessionSource::Cli => "cli".to_string(),
        AppGatewaySessionSource::VsCode => "vscode".to_string(),
        AppGatewaySessionSource::Exec => "exec".to_string(),
        AppGatewaySessionSource::AppGateway => "gateway".to_string(),
        AppGatewaySessionSource::Custom(value) => workspace_single_line(value),
        AppGatewaySessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. }) => {
            "subagent".to_string()
        }
        AppGatewaySessionSource::SubAgent(source) => format!("subagent {source}"),
        AppGatewaySessionSource::Unknown => "unknown".to_string(),
    }
}

pub(crate) fn refresh_workspace_subagent_summaries(rows: &mut [ThreadListRow]) {
    let children: Vec<WorkspaceSubagentChildSummary> = rows
        .iter()
        .filter_map(workspace_subagent_child_summary)
        .collect();
    let parent_indices: HashMap<ThreadId, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.thread_id, index))
        .collect();
    for row in rows.iter_mut() {
        row.subagents = WorkspaceSubagentSummary::default();
    }
    for child in children {
        let Some(parent_index) = parent_indices.get(&child.parent_thread_id).copied() else {
            continue;
        };
        let parent = &mut rows[parent_index];
        parent.subagents.total = parent.subagents.total.saturating_add(1);
        if child.is_open {
            parent.subagents.open = parent.subagents.open.saturating_add(1);
            if parent.subagents.labels.len() < 4 {
                parent.subagents.labels.push(child.label);
            }
        } else {
            parent.subagents.closed = parent.subagents.closed.saturating_add(1);
        }
        if child.is_running {
            parent.subagents.running = parent.subagents.running.saturating_add(1);
        }
    }
}

fn workspace_subagent_child_summary(row: &ThreadListRow) -> Option<WorkspaceSubagentChildSummary> {
    let parent_thread_id = row.subagent_parent_thread_id?;
    let status = workspace_subagent_status_label(row);
    let label = format!("{}  {status}", workspace_thread_display_name(row));
    Some(WorkspaceSubagentChildSummary {
        parent_thread_id,
        label: workspace_single_line(&label),
        is_open: !matches!(row.status, ThreadStatus::NotLoaded),
        is_running: workspace_status_has_running_flag(&row.status),
    })
}

fn workspace_subagent_status_label(row: &ThreadListRow) -> &'static str {
    match &row.status {
        ThreadStatus::Active { .. } if workspace_status_has_running_flag(&row.status) => "运行",
        ThreadStatus::Active { .. } => "活动",
        ThreadStatus::Idle => "空闲",
        ThreadStatus::SystemError => "错误",
        ThreadStatus::NotLoaded => "关闭",
    }
}

pub(crate) fn workspace_thread_display_name(row: &ThreadListRow) -> String {
    if matches!(row.source_kind, AppGatewaySessionSource::SubAgent(_)) {
        return subagent_display_name(
            row.thread_id,
            row.agent_base_name.as_deref(),
            row.agent_title.as_deref(),
            row.agent_display_name.as_deref(),
        );
    }
    row.name.clone()
}

pub(crate) fn parse_workspace_thread_id(thread_id: &str) -> Option<ThreadId> {
    ThreadId::from_string(thread_id).ok()
}

pub(crate) fn sort_workspace_thread_rows(
    rows: &mut [ThreadListRow],
    active_thread_id: Option<ThreadId>,
    pinned_thread_ids: &HashSet<ThreadId>,
) {
    rows.sort_by(|a, b| {
        let a_active = Some(a.thread_id) == active_thread_id;
        let b_active = Some(b.thread_id) == active_thread_id;
        let a_pinned = pinned_thread_ids.contains(&a.thread_id);
        let b_pinned = pinned_thread_ids.contains(&b.thread_id);
        b_active
            .cmp(&a_active)
            .then_with(|| b_pinned.cmp(&a_pinned))
            .then_with(|| workspace_row_priority(b).cmp(&workspace_row_priority(a)))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
}

fn workspace_fallback_thread_name(id: &str) -> String {
    format!("thread {}", id.chars().take(8).collect::<String>())
}

fn workspace_status_priority(status: &ThreadStatus) -> u8 {
    match status {
        ThreadStatus::Active { .. } => 2,
        ThreadStatus::Idle => 1,
        ThreadStatus::SystemError | ThreadStatus::NotLoaded => 0,
    }
}

fn workspace_row_priority(row: &ThreadListRow) -> u8 {
    if workspace_row_is_controlled(row) {
        return 3;
    }
    workspace_status_priority(&row.status)
}

pub(crate) fn workspace_row_is_controlled(row: &ThreadListRow) -> bool {
    row.control_state.is_some() || workspace_status_has_controlled_flag(&row.status)
}

pub(in crate::workspace) fn workspace_row_is_closed(row: &ThreadListRow) -> bool {
    matches!(row.status, ThreadStatus::NotLoaded)
}

pub(in crate::workspace) fn workspace_status_has_controlled_flag(status: &ThreadStatus) -> bool {
    matches!(
        status,
        ThreadStatus::Active { active_flags }
            if active_flags.contains(&ThreadActiveFlag::Controlled)
    )
}

pub(in crate::workspace) fn workspace_status_has_running_flag(status: &ThreadStatus) -> bool {
    matches!(
        status,
        ThreadStatus::Active { active_flags }
            if active_flags.contains(&ThreadActiveFlag::Running)
    )
}

pub(crate) fn workspace_status_without_control(status: &ThreadStatus) -> ThreadStatus {
    match status {
        ThreadStatus::Active { active_flags } => {
            let active_flags: Vec<ThreadActiveFlag> = active_flags
                .iter()
                .copied()
                .filter(|flag| *flag != ThreadActiveFlag::Controlled)
                .collect();
            if active_flags.is_empty() {
                ThreadStatus::Idle
            } else {
                ThreadStatus::Active { active_flags }
            }
        }
        status => status.clone(),
    }
}

pub(crate) fn workspace_row_should_auto_observe(row: &ThreadListRow) -> bool {
    workspace_row_is_controlled(row) || matches!(row.status, ThreadStatus::Active { .. })
}

pub(crate) fn workspace_single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
