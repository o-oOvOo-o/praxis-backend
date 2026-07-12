use praxis_app_gateway_protocol::THREAD_HISTORY_MAX_PAGE_SIZE;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadHistoryBuilder;
use praxis_app_gateway_protocol::ThreadHistoryCursor;
use praxis_app_gateway_protocol::ThreadHistoryPage;
use praxis_app_gateway_protocol::ThreadHistoryRange;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::build_recent_turns_from_rollout_items;
use praxis_app_gateway_protocol::build_turns_from_rollout_items;
use praxis_core::RolloutRecorder;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use std::path::Path;

#[derive(Debug)]
pub(in crate::praxis_message_processor) enum ThreadHistoryPageReadError {
    Io(std::io::Error),
    InvalidCursor { before_turn: u64, total_turns: u64 },
    InvalidLimit { limit: u32 },
}

impl std::fmt::Display for ThreadHistoryPageReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => std::fmt::Display::fmt(error, formatter),
            Self::InvalidCursor {
                before_turn,
                total_turns,
            } => write!(
                formatter,
                "history cursor beforeTurn {before_turn} exceeds totalTurns {total_turns}"
            ),
            Self::InvalidLimit { limit } => write!(
                formatter,
                "history page limit must be between 1 and {THREAD_HISTORY_MAX_PAGE_SIZE}, got {limit}"
            ),
        }
    }
}

impl std::error::Error for ThreadHistoryPageReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidCursor { .. } | Self::InvalidLimit { .. } => None,
        }
    }
}

impl From<std::io::Error> for ThreadHistoryPageReadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub(in crate::praxis_message_processor) struct ThreadTurnPage {
    pub(in crate::praxis_message_processor) turns: Vec<Turn>,
    pub(in crate::praxis_message_processor) page: ThreadHistoryPage,
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::praxis_message_processor) struct ThreadTurnHydration {
    turn_limit: Option<usize>,
}

impl ThreadTurnHydration {
    pub(in crate::praxis_message_processor) fn all() -> Self {
        Self { turn_limit: None }
    }

    pub(in crate::praxis_message_processor) fn recent(turn_limit: Option<usize>) -> Self {
        Self { turn_limit }
    }

    fn build_turns(self, items: &[RolloutItem]) -> Vec<Turn> {
        match self.turn_limit {
            Some(turn_limit) => build_recent_turns_from_rollout_items(items, turn_limit),
            None => build_turns_from_rollout_items(items),
        }
    }

    fn builder(self) -> ThreadHistoryBuilder {
        match self.turn_limit {
            Some(turn_limit) => ThreadHistoryBuilder::with_max_finished_turns(turn_limit),
            None => ThreadHistoryBuilder::new(),
        }
    }
}

pub(in crate::praxis_message_processor) enum ThreadHistorySource<'a> {
    RolloutPath(&'a Path),
    RolloutItems(&'a [RolloutItem]),
}

pub(super) fn preview_from_rollout_items(items: &[RolloutItem]) -> String {
    items
        .iter()
        .find_map(praxis_state::thread_preview::rollout_item_preview)
        .map(praxis_state::thread_preview::ThreadUserPreview::into_display_text)
        .unwrap_or_default()
}

pub(super) async fn read_thread_rollout_items(path: &Path) -> std::io::Result<Vec<RolloutItem>> {
    let items = match read_thread_initial_history(path).await? {
        InitialHistory::New => Vec::new(),
        InitialHistory::Forked(items) => items,
        InitialHistory::Resumed(resumed) => resumed.history,
    };

    Ok(items)
}

pub(super) async fn read_thread_initial_history(path: &Path) -> std::io::Result<InitialHistory> {
    RolloutRecorder::get_rollout_history(path).await
}

pub(super) async fn read_thread_turns_from_rollout(
    path: &Path,
    hydration: ThreadTurnHydration,
) -> std::io::Result<Vec<Turn>> {
    let mut builder = hydration.builder();
    RolloutRecorder::scan_rollout_items(path, |item| {
        builder.handle_rollout_item(&item);
    })
    .await?;
    Ok(builder.finish())
}

pub(super) async fn read_thread_turn_page_from_rollout(
    path: &Path,
    cursor: Option<ThreadHistoryCursor>,
    limit: u32,
) -> Result<ThreadTurnPage, ThreadHistoryPageReadError> {
    validate_turn_page_limit(limit)?;

    let mut builder = ThreadHistoryBuilder::new();
    RolloutRecorder::scan_rollout_items(path, |item| {
        builder.handle_rollout_item(&item);
    })
    .await?;
    select_turn_page(builder.finish(), cursor, limit)
}

fn select_turn_page(
    turns: Vec<Turn>,
    cursor: Option<ThreadHistoryCursor>,
    limit: u32,
) -> Result<ThreadTurnPage, ThreadHistoryPageReadError> {
    validate_turn_page_limit(limit)?;
    let total_turns = turns.len();
    let end_turn = match cursor {
        Some(cursor) => usize::try_from(cursor.before_turn).unwrap_or(usize::MAX),
        None => total_turns,
    };
    if end_turn > total_turns {
        return Err(ThreadHistoryPageReadError::InvalidCursor {
            before_turn: cursor.map_or(u64::MAX, |cursor| cursor.before_turn),
            total_turns: u64::try_from(total_turns).unwrap_or(u64::MAX),
        });
    }

    let start_turn = end_turn.saturating_sub(limit as usize);
    let page_turns = turns
        .into_iter()
        .skip(start_turn)
        .take(end_turn - start_turn)
        .collect();
    let older_cursor = (start_turn > 0).then_some(ThreadHistoryCursor {
        before_turn: u64::try_from(start_turn).unwrap_or(u64::MAX),
    });
    Ok(ThreadTurnPage {
        turns: page_turns,
        page: ThreadHistoryPage {
            range: ThreadHistoryRange {
                start_turn: u64::try_from(start_turn).unwrap_or(u64::MAX),
                end_turn: u64::try_from(end_turn).unwrap_or(u64::MAX),
                total_turns: u64::try_from(total_turns).unwrap_or(u64::MAX),
            },
            older_cursor,
        },
    })
}

fn validate_turn_page_limit(limit: u32) -> Result<(), ThreadHistoryPageReadError> {
    if limit == 0 || limit > THREAD_HISTORY_MAX_PAGE_SIZE {
        return Err(ThreadHistoryPageReadError::InvalidLimit { limit });
    }
    Ok(())
}

pub(super) async fn hydrate_thread_turns(
    thread: &mut Thread,
    source: ThreadHistorySource<'_>,
    hydration: ThreadTurnHydration,
    active_turn: Option<&Turn>,
) -> std::result::Result<(), String> {
    let mut turns = match source {
        ThreadHistorySource::RolloutPath(rollout_path) => {
            read_thread_turns_from_rollout(rollout_path, hydration)
                .await
                .map_err(|err| {
                    format!(
                        "failed to load rollout `{}` for thread {}: {err}",
                        rollout_path.display(),
                        thread.id
                    )
                })?
        }
        ThreadHistorySource::RolloutItems(items) => hydration.build_turns(items),
    };
    if let Some(active_turn) = active_turn {
        merge_active_turn(&mut turns, active_turn.clone());
    }
    thread.turns = turns;
    Ok(())
}

fn merge_active_turn(turns: &mut Vec<Turn>, active_turn: Turn) {
    turns.retain(|turn| turn.id != active_turn.id);
    turns.push(active_turn);
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_app_gateway_protocol::ThreadItem;
    use praxis_app_gateway_protocol::TurnStatus;
    use praxis_app_gateway_protocol::UserInput;
    use std::collections::HashSet;

    fn typed_turn(id: &str) -> Turn {
        Turn {
            id: id.to_string(),
            status: TurnStatus::Completed,
            error: None,
            items: vec![
                ThreadItem::UserMessage {
                    id: format!("user-{id}"),
                    content: vec![UserInput::Text {
                        text: format!("question {id}"),
                        text_elements: Vec::new(),
                    }],
                },
                ThreadItem::AgentMessage {
                    id: format!("agent-{id}"),
                    text: format!("answer {id}"),
                    phase: None,
                    memory_citation: None,
                },
            ],
        }
    }

    fn history(ids: &[&str]) -> Vec<Turn> {
        ids.iter().map(|id| typed_turn(id)).collect()
    }

    #[test]
    fn pages_older_turns_without_duplicates_or_gaps() {
        let complete = history(&["a", "b", "c", "d", "e"]);
        let mut page = select_turn_page(complete.clone(), None, 2).expect("latest page");
        let mut assembled = page.turns;

        while let Some(cursor) = page.page.older_cursor {
            page = select_turn_page(complete.clone(), Some(cursor), 2).expect("older page");
            let mut older = page.turns;
            older.append(&mut assembled);
            assembled = older;
        }

        assert_eq!(assembled, complete);
        let ids = assembled
            .iter()
            .map(|turn| turn.id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), assembled.len());
    }

    #[test]
    fn oldest_based_cursor_stays_stable_when_new_turns_are_appended() {
        let original = history(&["a", "b", "c", "d"]);
        let latest = select_turn_page(original.clone(), None, 2).expect("latest page");
        let cursor = latest.page.older_cursor.expect("older cursor");

        let appended = history(&["a", "b", "c", "d", "e"]);
        let older = select_turn_page(appended, Some(cursor), 2).expect("stable older page");
        let mut assembled = older.turns;
        assembled.extend(latest.turns);

        assert_eq!(assembled, original);
    }

    #[test]
    fn page_range_and_empty_boundaries_are_deterministic() {
        let latest = select_turn_page(history(&["a", "b", "c"]), None, 2).expect("latest");
        assert_eq!(
            latest.page.range,
            ThreadHistoryRange {
                start_turn: 1,
                end_turn: 3,
                total_turns: 3,
            }
        );
        assert_eq!(
            latest.page.older_cursor,
            Some(ThreadHistoryCursor { before_turn: 1 })
        );

        let empty = select_turn_page(Vec::new(), None, 2).expect("empty history");
        assert!(empty.turns.is_empty());
        assert_eq!(empty.page.range.start_turn, 0);
        assert_eq!(empty.page.range.end_turn, 0);
        assert_eq!(empty.page.range.total_turns, 0);
        assert_eq!(empty.page.older_cursor, None);

        let boundary = select_turn_page(
            history(&["a", "b"]),
            Some(ThreadHistoryCursor { before_turn: 0 }),
            2,
        )
        .expect("oldest boundary");
        assert!(boundary.turns.is_empty());
        assert_eq!(boundary.page.older_cursor, None);
    }

    #[test]
    fn page_preserves_typed_turn_items() {
        let page = select_turn_page(history(&["typed"]), None, 1).expect("typed page");
        assert!(matches!(
            &page.turns[0].items[0],
            ThreadItem::UserMessage { content, .. }
                if matches!(&content[0], UserInput::Text { text, .. } if text == "question typed")
        ));
        assert!(matches!(
            &page.turns[0].items[1],
            ThreadItem::AgentMessage { text, .. } if text == "answer typed"
        ));
    }

    #[test]
    fn rejects_non_advancing_limits_and_out_of_range_cursors() {
        assert!(matches!(
            select_turn_page(history(&["a"]), None, 0),
            Err(ThreadHistoryPageReadError::InvalidLimit { limit: 0 })
        ));
        assert!(matches!(
            select_turn_page(
                history(&["a"]),
                Some(ThreadHistoryCursor { before_turn: 2 }),
                1,
            ),
            Err(ThreadHistoryPageReadError::InvalidCursor {
                before_turn: 2,
                total_turns: 1,
            })
        ));
    }
}
