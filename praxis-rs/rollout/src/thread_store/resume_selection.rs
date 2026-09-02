use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::protocol::RolloutItem;
use praxis_utils_path as path_utils;

use crate::list::ThreadsPage;
use crate::metadata;

pub(crate) async fn filter_fs_page_by_cwd(
    mut page: ThreadsPage,
    cwd: Option<&Path>,
    default_provider: &str,
) -> ThreadsPage {
    let Some(cwd) = cwd else {
        return page;
    };

    let mut filtered = Vec::with_capacity(page.items.len());
    for item in page.items {
        if resume_candidate_matches_cwd(
            item.path.as_path(),
            item.cwd.as_deref(),
            cwd,
            default_provider,
        )
        .await
        {
            filtered.push(item);
        }
    }
    page.items = filtered;
    page
}

pub(super) async fn select_resume_path(
    page: &ThreadsPage,
    filter_cwd: Option<&Path>,
    default_provider: &str,
) -> Option<PathBuf> {
    match filter_cwd {
        Some(cwd) => {
            for item in &page.items {
                if resume_candidate_matches_cwd(
                    item.path.as_path(),
                    item.cwd.as_deref(),
                    cwd,
                    default_provider,
                )
                .await
                {
                    return Some(item.path.clone());
                }
            }
            None
        }
        None => page.items.first().map(|item| item.path.clone()),
    }
}

pub(crate) async fn resume_candidate_matches_cwd(
    rollout_path: &Path,
    cached_cwd: Option<&Path>,
    cwd: &Path,
    default_provider: &str,
) -> bool {
    if cached_cwd.is_some_and(|session_cwd| cwd_matches(session_cwd, cwd)) {
        return true;
    }

    let mut latest_turn_context_cwd = None;
    if crate::thread_store::scan_items(rollout_path, |item| {
        if let RolloutItem::TurnContext(turn_context) = item {
            latest_turn_context_cwd = Some(turn_context.cwd);
        }
    })
    .await
    .is_ok()
        && let Some(latest_turn_context_cwd) = latest_turn_context_cwd
    {
        return cwd_matches(latest_turn_context_cwd.as_path(), cwd);
    }

    metadata::extract_metadata_from_rollout(rollout_path, default_provider)
        .await
        .is_ok_and(|outcome| cwd_matches(outcome.metadata.cwd.as_path(), cwd))
}

pub(crate) fn cwd_matches(session_cwd: &Path, cwd: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (
        path_utils::normalize_for_path_comparison(session_cwd),
        path_utils::normalize_for_path_comparison(cwd),
    ) {
        return ca == cb || ca.starts_with(&cb);
    }
    session_cwd == cwd || session_cwd.starts_with(cwd)
}
