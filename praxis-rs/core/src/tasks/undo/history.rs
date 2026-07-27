use praxis_protocol::models::ResponseItem;
use praxis_protocol::workspace_history::WorkspaceCheckpointRef;

pub(super) fn find_latest_workspace_checkpoint(
    items: &[ResponseItem],
) -> Option<(usize, WorkspaceCheckpointRef)> {
    items
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, item)| match item {
            ResponseItem::WorkspaceCheckpoint { checkpoint } => Some((index, checkpoint.clone())),
            _ => None,
        })
}

pub(super) fn find_previous_workspace_checkpoint(
    items: &[ResponseItem],
    before_index: usize,
) -> Option<WorkspaceCheckpointRef> {
    items[..before_index]
        .iter()
        .rev()
        .find_map(|item| match item {
            ResponseItem::WorkspaceCheckpoint { checkpoint } => Some(checkpoint.clone()),
            _ => None,
        })
}
