use super::*;

pub(crate) enum ThreadHistorySource<'a> {
    RolloutPath(&'a Path),
    RolloutItems(&'a [RolloutItem]),
}

pub(crate) async fn read_thread_rollout_items(path: &Path) -> std::io::Result<Vec<RolloutItem>> {
    let items = match RolloutRecorder::get_rollout_history(path).await? {
        InitialHistory::New => Vec::new(),
        InitialHistory::Forked(items) => items,
        InitialHistory::Resumed(resumed) => resumed.history,
    };

    Ok(items)
}

pub(crate) async fn read_thread_turns_from_rollout(path: &Path) -> std::io::Result<Vec<Turn>> {
    read_thread_rollout_items(path)
        .await
        .map(|items| build_turns_from_rollout_items(&items))
}

pub(crate) async fn hydrate_thread_turns(
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
