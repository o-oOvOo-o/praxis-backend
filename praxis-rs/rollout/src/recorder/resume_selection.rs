use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::protocol::RolloutItem;
use praxis_utils_path as path_utils;

use crate::list::ThreadsPage;
use crate::metadata;
use crate::recorder::RolloutRecorder;

pub(super) async fn filter_fs_page_by_cwd(
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

pub(super) async fn select_resume_path_from_db_page(
    page: &praxis_state::ThreadsPage,
    filter_cwd: Option<&Path>,
    default_provider: &str,
) -> Option<PathBuf> {
    match filter_cwd {
        Some(cwd) => {
            for item in &page.items {
                if resume_candidate_matches_cwd(
                    item.rollout_path.as_path(),
                    Some(item.cwd.as_path()),
                    cwd,
                    default_provider,
                )
                .await
                {
                    return Some(item.rollout_path.clone());
                }
            }
            None
        }
        None => page.items.first().map(|item| item.rollout_path.clone()),
    }
}

async fn resume_candidate_matches_cwd(
    rollout_path: &Path,
    cached_cwd: Option<&Path>,
    cwd: &Path,
    default_provider: &str,
) -> bool {
    if cached_cwd.is_some_and(|session_cwd| cwd_matches(session_cwd, cwd)) {
        return true;
    }

    if let Ok((items, _, _)) = RolloutRecorder::load_rollout_items(rollout_path).await
        && let Some(latest_turn_context_cwd) = items.iter().rev().find_map(|item| match item {
            RolloutItem::TurnContext(turn_context) => Some(turn_context.cwd.as_path()),
            RolloutItem::SessionMeta(_)
            | RolloutItem::ResponseItem(_)
            | RolloutItem::Compacted(_)
            | RolloutItem::EventMsg(_) => None,
        })
    {
        return cwd_matches(latest_turn_context_cwd, cwd);
    }

    metadata::extract_metadata_from_rollout(rollout_path, default_provider)
        .await
        .is_ok_and(|outcome| cwd_matches(outcome.metadata.cwd.as_path(), cwd))
}

fn cwd_matches(session_cwd: &Path, cwd: &Path) -> bool {
    if let (Ok(ca), Ok(cb)) = (
        path_utils::normalize_for_path_comparison(session_cwd),
        path_utils::normalize_for_path_comparison(cwd),
    ) {
        return ca == cb || ca.starts_with(&cb);
    }
    session_cwd == cwd || session_cwd.starts_with(cwd)
}
