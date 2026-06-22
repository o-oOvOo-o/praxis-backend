use praxis_git_utils::GhostCommit;
use praxis_protocol::models::ResponseItem;

pub(super) fn find_latest_ghost_snapshot(items: &[ResponseItem]) -> Option<(usize, GhostCommit)> {
    items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(idx, item)| match item {
            ResponseItem::GhostSnapshot { ghost_commit } => Some((idx, ghost_commit.clone())),
            _ => None,
        })
}
