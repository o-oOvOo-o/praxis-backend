use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadHistoryBuilder;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::build_recent_turns_from_rollout_items;
use praxis_app_gateway_protocol::build_turns_from_rollout_items;
use praxis_core::RolloutRecorder;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use std::path::Path;

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
