use praxis_app_gateway_protocol::ResumedHistoryLabel;
use praxis_app_gateway_protocol::ResumedHistoryLane;
use praxis_app_gateway_protocol::ResumedThreadHistoryAction;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::classify_resumed_thread_item;
use unicode_segmentation::UnicodeSegmentation;

use crate::history_cell::ChatLane;
use crate::history_cell::ResumeReplayHistoryCell;

const RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES: usize = 180;
const RESUME_REPLAY_TRUNCATION_SUFFIX: &str = "...";

#[derive(Default)]
pub(super) struct ResumeReplayProjector {
    summary: FoldedToolSummary,
}

impl ResumeReplayProjector {
    pub(super) fn project_items(
        &mut self,
        items: impl IntoIterator<Item = ThreadItem>,
        mut emit: impl FnMut(ResumeReplayHistoryCell),
    ) {
        for item in items {
            self.project_item(item, &mut emit);
        }
    }

    pub(super) fn project_single(item: ThreadItem, mut emit: impl FnMut(ResumeReplayHistoryCell)) {
        let mut projector = Self::default();
        projector.project_item(item, &mut emit);
        projector.finish(emit);
    }

    fn project_item(&mut self, item: ThreadItem, emit: &mut impl FnMut(ResumeReplayHistoryCell)) {
        match project_item(item) {
            ResumeReplayItem::Show(cell) => {
                if let Some(summary_cell) = self.summary.take_history_cell() {
                    emit(summary_cell);
                }
                emit(cell);
            }
            ResumeReplayItem::FoldToolEvent => {
                self.summary.record_folded_tool_event();
            }
            ResumeReplayItem::Drop => {}
        }
    }

    pub(super) fn finish(self, mut emit: impl FnMut(ResumeReplayHistoryCell)) {
        if let Some(cell) = self.summary.into_history_cell() {
            emit(cell);
        }
    }
}

#[derive(Default)]
struct FoldedToolSummary {
    folded_tool_events: usize,
}

impl FoldedToolSummary {
    fn record_folded_tool_event(&mut self) {
        self.folded_tool_events = self.folded_tool_events.saturating_add(1);
    }

    fn take_history_cell(&mut self) -> Option<ResumeReplayHistoryCell> {
        let folded_tool_events = std::mem::take(&mut self.folded_tool_events);
        folded_tool_events_history_cell(folded_tool_events)
    }

    fn into_history_cell(self) -> Option<ResumeReplayHistoryCell> {
        folded_tool_events_history_cell(self.folded_tool_events)
    }
}

enum ResumeReplayItem {
    Show(ResumeReplayHistoryCell),
    FoldToolEvent,
    Drop,
}

fn project_item(item: ThreadItem) -> ResumeReplayItem {
    match classify_resumed_thread_item(item) {
        ResumedThreadHistoryAction::Show {
            lane,
            label,
            preview,
        } => ResumeReplayItem::Show(ResumeReplayHistoryCell::new(
            lane_to_chat_lane(lane),
            label_text(label).to_string(),
            compact_preview(&preview),
        )),
        ResumedThreadHistoryAction::FoldToolEvent => ResumeReplayItem::FoldToolEvent,
        ResumedThreadHistoryAction::Drop => ResumeReplayItem::Drop,
    }
}

fn folded_tool_events_history_cell(count: usize) -> Option<ResumeReplayHistoryCell> {
    (count > 0).then(|| {
        ResumeReplayHistoryCell::new(
            ChatLane::Assistant,
            "Resume".to_string(),
            folded_tool_events_preview(count),
        )
    })
}

fn folded_tool_events_preview(count: usize) -> String {
    match count {
        1 => "1 tool event hidden from resumed history".to_string(),
        count => format!("{count} tool events hidden from resumed history"),
    }
}

fn lane_to_chat_lane(lane: ResumedHistoryLane) -> ChatLane {
    match lane {
        ResumedHistoryLane::User => ChatLane::User,
        ResumedHistoryLane::Assistant => ChatLane::Assistant,
    }
}

fn label_text(label: ResumedHistoryLabel) -> &'static str {
    match label {
        ResumedHistoryLabel::You => "You",
        ResumedHistoryLabel::Assistant => "Assistant",
        ResumedHistoryLabel::AssistantNote => "Note",
        ResumedHistoryLabel::Plan => "Plan",
        ResumedHistoryLabel::Reasoning => "Reasoning",
        ResumedHistoryLabel::Review => "Review",
        ResumedHistoryLabel::ReviewExited => "Review done",
        ResumedHistoryLabel::Context => "Context",
        ResumedHistoryLabel::Hook => "Hook",
    }
}

fn compact_preview(text: &str) -> String {
    let mut result = String::with_capacity(RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES);
    let mut graphemes = 0usize;
    let mut last_was_space = true;
    let mut truncated = false;

    for grapheme in text.graphemes(true) {
        if grapheme.chars().all(char::is_whitespace) {
            if !last_was_space
                && !result.is_empty()
                && graphemes < RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES
            {
                result.push(' ');
                graphemes += 1;
                last_was_space = true;
            }
            continue;
        }

        if graphemes >= RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES {
            truncated = true;
            break;
        }

        result.push_str(grapheme);
        graphemes += 1;
        last_was_space = false;
    }

    while result.ends_with(' ') {
        result.pop();
    }

    if truncated {
        let content_budget = RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES
            .saturating_sub(RESUME_REPLAY_TRUNCATION_SUFFIX.graphemes(true).count());
        result = result.graphemes(true).take(content_budget).collect();
        while result.ends_with(' ') {
            result.pop();
        }
        result.push_str(RESUME_REPLAY_TRUNCATION_SUFFIX);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_preview_normalizes_whitespace() {
        assert_eq!(compact_preview("  first\n\tsecond  "), "first second");
    }

    #[test]
    fn compact_preview_truncates_to_budget_with_suffix() {
        let preview = "a".repeat(RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES + 1);
        let compact = compact_preview(&preview);

        assert_eq!(
            compact.graphemes(true).count(),
            RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES
        );
        assert!(compact.ends_with(RESUME_REPLAY_TRUNCATION_SUFFIX));
    }

    #[test]
    fn compact_preview_keeps_exact_budget_without_suffix() {
        let preview = "a".repeat(RESUME_REPLAY_PREVIEW_MAX_GRAPHEMES);

        assert_eq!(compact_preview(&preview), preview);
    }

    #[test]
    fn folded_tool_summary_uses_singular_copy() {
        assert_eq!(
            folded_tool_events_preview(1),
            "1 tool event hidden from resumed history"
        );
    }

    #[test]
    fn folded_tool_summary_uses_plural_copy() {
        assert_eq!(
            folded_tool_events_preview(3),
            "3 tool events hidden from resumed history"
        );
    }

    #[test]
    fn folded_tool_summary_take_resets_pending_count() {
        let mut summary = FoldedToolSummary::default();
        summary.record_folded_tool_event();
        summary.record_folded_tool_event();

        assert!(summary.take_history_cell().is_some());
        assert!(summary.take_history_cell().is_none());
    }
}
