use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::build_turns_from_rollout_items;
use praxis_core::RolloutRecorder;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::RolloutItem;
use std::path::Path;

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

pub(super) async fn read_thread_turns_from_rollout(path: &Path) -> std::io::Result<Vec<Turn>> {
    read_thread_rollout_items(path)
        .await
        .map(|items| build_turns_from_rollout_items(&items))
}

pub(super) async fn hydrate_thread_turns(
    thread: &mut Thread,
    source: ThreadHistorySource<'_>,
    active_turn: Option<&Turn>,
) -> std::result::Result<(), String> {
    let mut turns = match source {
        ThreadHistorySource::RolloutPath(rollout_path) => {
            read_thread_turns_from_rollout(rollout_path)
                .await
                .map_err(|err| {
                    format!(
                        "failed to load rollout `{}` for thread {}: {err}",
                        rollout_path.display(),
                        thread.id
                    )
                })?
        }
        ThreadHistorySource::RolloutItems(items) => build_turns_from_rollout_items(items),
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
