use super::*;

pub(super) const REVIEW_FALLBACK_MESSAGE: &str = "Reviewer failed to output a response.";

pub(super) fn render_review_output_text(output: &ReviewOutputEvent) -> String {
    let explanation = output.overall_explanation.trim();
    if explanation.is_empty() {
        REVIEW_FALLBACK_MESSAGE.to_string()
    } else {
        explanation.to_string()
    }
}

pub fn convert_patch_changes(
    changes: &HashMap<std::path::PathBuf, praxis_protocol::protocol::FileChange>,
) -> Vec<FileUpdateChange> {
    let mut converted: Vec<FileUpdateChange> = changes
        .iter()
        .map(|(path, change)| FileUpdateChange {
            path: path.to_string_lossy().into_owned(),
            kind: map_patch_change_kind(change),
            diff: format_file_change_diff(change),
        })
        .collect();
    converted.sort_by(|a, b| a.path.cmp(&b.path));
    converted
}

pub(super) fn convert_dynamic_tool_content_items(
    items: &[praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem],
) -> Vec<DynamicToolCallOutputContentItem> {
    items
        .iter()
        .cloned()
        .map(|item| match item {
            praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem::InputText {
                text,
            } => DynamicToolCallOutputContentItem::InputText { text },
            praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem::InputImage {
                image_url,
            } => DynamicToolCallOutputContentItem::InputImage { image_url },
        })
        .collect()
}

fn map_patch_change_kind(change: &praxis_protocol::protocol::FileChange) -> PatchChangeKind {
    match change {
        praxis_protocol::protocol::FileChange::Add { .. } => PatchChangeKind::Add,
        praxis_protocol::protocol::FileChange::Delete { .. } => PatchChangeKind::Delete,
        praxis_protocol::protocol::FileChange::Update { move_path, .. } => {
            PatchChangeKind::Update {
                move_path: move_path.clone(),
            }
        }
    }
}

fn format_file_change_diff(change: &praxis_protocol::protocol::FileChange) -> String {
    match change {
        praxis_protocol::protocol::FileChange::Add { content } => content.clone(),
        praxis_protocol::protocol::FileChange::Delete { content } => content.clone(),
        praxis_protocol::protocol::FileChange::Update {
            unified_diff,
            move_path,
        } => {
            if let Some(path) = move_path {
                format!("{unified_diff}\n\nMoved to: {}", path.display())
            } else {
                unified_diff.clone()
            }
        }
    }
}
