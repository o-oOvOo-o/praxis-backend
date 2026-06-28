use super::*;

#[derive(Debug, Clone, Default)]
pub(super) struct PluginListFetchState {
    pub(super) cache_cwd: Option<PathBuf>,
    pub(super) in_flight_cwd: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(super) struct PluginInstallAuthFlowState {
    pub(super) plugin_display_name: String,
    pub(super) next_app_index: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExternalEditorState {
    #[default]
    Closed,
    Requested,
    Active,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StatusIndicatorState {
    pub(super) header: String,
    pub(super) details: Option<String>,
    pub(super) details_max_lines: usize,
    pub(super) thinking_persona: ThinkingPersona,
}

impl StatusIndicatorState {
    pub(super) fn turn_running() -> Self {
        Self {
            header: GENERIC_STATUS_HEADER.to_string(),
            details: None,
            details_max_lines: STATUS_DETAILS_DEFAULT_MAX_LINES,
            thinking_persona: ThinkingPersona::None,
        }
    }

    pub(super) fn is_guardian_review(&self) -> bool {
        self.header == "Reviewing approval request" || self.header.starts_with("Reviewing ")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReasoningBlockKind {
    Summary,
    Full,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PendingGuardianReviewStatus {
    entries: Vec<PendingGuardianReviewStatusEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingGuardianReviewStatusEntry {
    id: String,
    detail: String,
}

impl PendingGuardianReviewStatus {
    pub(super) fn start_or_update(&mut self, id: String, detail: String) {
        if let Some(existing) = self.entries.iter_mut().find(|entry| entry.id == id) {
            existing.detail = detail;
        } else {
            self.entries
                .push(PendingGuardianReviewStatusEntry { id, detail });
        }
    }

    pub(super) fn finish(&mut self, id: &str) -> bool {
        let original_len = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != original_len
    }

    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // Guardian review status is derived from the full set of currently pending
    // review entries. The generic status cache on `ChatWidget` stores whichever
    // footer is currently rendered; this helper computes the guardian-specific
    // footer snapshot that should replace it while reviews remain in flight.
    pub(super) fn status_indicator_state(&self) -> Option<StatusIndicatorState> {
        let details = if self.entries.len() == 1 {
            self.entries.first().map(|entry| entry.detail.clone())
        } else if self.entries.is_empty() {
            None
        } else {
            let mut lines = self
                .entries
                .iter()
                .take(3)
                .map(|entry| format!("• {}", entry.detail))
                .collect::<Vec<_>>();
            let remaining = self.entries.len().saturating_sub(3);
            if remaining > 0 {
                lines.push(format!("+{remaining} more"));
            }
            Some(lines.join("\n"))
        };
        let details = details?;
        let header = if self.entries.len() == 1 {
            String::from("Reviewing approval request")
        } else {
            format!("Reviewing {} approval requests", self.entries.len())
        };
        let details_max_lines = if self.entries.len() == 1 { 1 } else { 4 };
        Some(StatusIndicatorState {
            header,
            details: Some(details),
            details_max_lines,
            thinking_persona: ThinkingPersona::None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActiveCellRenderCacheKey {
    pub(super) width: u16,
    pub(super) revision: u64,
    pub(super) animation_tick: Option<u64>,
    pub(super) presentation_revision: u64,
}

#[derive(Clone, Debug)]
pub(super) struct ActiveCellRenderCache {
    pub(super) key: ActiveCellRenderCacheKey,
    pub(super) lines: Vec<Line<'static>>,
    pub(super) desired_height: u16,
    pub(super) mouse_targets: Vec<history_cell::HistoryCellMouseTarget>,
}

#[derive(Clone, Debug)]
pub(super) struct WorkspaceActiveTailCache {
    pub(super) key: ActiveCellRenderCacheKey,
    pub(super) lane: ChatLane,
    pub(super) lines: Vec<Line<'static>>,
    pub(super) mouse_targets: Vec<history_cell::HistoryCellMouseTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceReasoningChoice {
    pub(super) effort: Option<ReasoningEffortConfig>,
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) is_current: bool,
}

pub(super) fn thread_control_display_label(control_state: &ThreadControlState) -> String {
    let controller = match (control_state.controller.kind, control_state.controller.rank) {
        (ThreadControllerKind::External, Some(rank)) => format!("external/R{rank}"),
        (ThreadControllerKind::External, None) => "external".to_string(),
        (ThreadControllerKind::Thread, Some(rank)) => format!("R{rank}"),
        (ThreadControllerKind::Thread, None) => "thread".to_string(),
    };
    let label = control_state
        .controller
        .label
        .as_deref()
        .unwrap_or(control_state.controller.id.as_str());
    if label.eq_ignore_ascii_case(&controller) {
        controller
    } else {
        format!("{controller}:{label}")
    }
}

/// Cached nickname and role for a collab agent thread, used to attach human-readable labels to
/// rendered tool-call items.
///
/// Populated externally by `App` via `set_collab_agent_metadata` and consulted by the
/// notification-to-core-event conversion helpers. Defaults to empty so that missing metadata
/// degrades to the previous behavior of showing raw thread ids.
#[derive(Clone, Debug, Default)]
pub(super) struct CollabAgentMetadata {
    pub(super) agent_base_name: Option<String>,
    pub(super) agent_title: Option<String>,
    pub(super) agent_display_name: Option<String>,
    pub(super) agent_role: Option<String>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum PraxisOpTarget {
    Direct(UnboundedSender<Op>),
    AppEvent,
}

/// Snapshot of active-cell state that affects transcript overlay rendering.
///
/// The overlay keeps a cached "live tail" for the in-flight cell; this key lets
/// it cheaply decide when to recompute that tail as the active cell evolves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActiveCellTranscriptKey {
    /// Cache-busting revision for in-place updates.
    ///
    /// Many active cells are updated incrementally while streaming (for example when exec groups
    /// add output or change status), and the transcript overlay caches its live tail, so this
    /// revision gives a cheap way to say "same active cell, but its transcript output is different
    /// now". Callers bump it on any mutation that can affect `HistoryCell::transcript_lines`.
    pub(crate) revision: u64,
    /// Whether the active cell continues the prior stream, which affects
    /// spacing between transcript blocks.
    pub(crate) is_stream_continuation: bool,
    /// Optional animation tick for time-dependent transcript output.
    ///
    /// When this changes, the overlay recomputes the cached tail even if the revision and width
    /// are unchanged, which is how shimmer/spinner visuals can animate in the overlay without any
    /// underlying data change.
    pub(crate) animation_tick: Option<u64>,
    /// Global history-presentation revision for fold/expand state.
    pub(crate) presentation_revision: u64,
}
