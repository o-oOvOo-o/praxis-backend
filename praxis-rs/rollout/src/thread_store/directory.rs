use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use chrono::SecondsFormat;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use serde_json::Value;
use time::format_description::well_known::Rfc3339;

use crate::ARCHIVED_SESSIONS_SUBDIR;
use crate::RolloutConfigView;
use crate::list::Cursor;
use crate::list::ThreadItem;
use crate::list::ThreadListConfig;
use crate::list::ThreadListLayout;
use crate::list::ThreadSortKey;
use crate::list::ThreadsPage;
use crate::list::get_threads;
use crate::list::get_threads_in_root;
use crate::list::parse_cursor;
use crate::list::parse_timestamp_uuid_from_filename;
use crate::state_db;

use super::resume_selection::filter_fs_page_by_cwd;

pub(super) async fn find_rollout_paths(
    root: &Path,
    thread_ids: &HashSet<ThreadId>,
) -> std::io::Result<HashMap<ThreadId, PathBuf>> {
    if thread_ids.is_empty() || !tokio::fs::try_exists(root).await? {
        return Ok(HashMap::new());
    }
    let mut directories = vec![root.to_path_buf()];
    let mut found = HashMap::with_capacity(thread_ids.len());
    while let Some(directory) = directories.pop() {
        let mut entries = tokio::fs::read_dir(directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                directories.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let Some((_, uuid)) = parse_timestamp_uuid_from_filename(file_name) else {
                continue;
            };
            let Ok(thread_id) = ThreadId::from_string(&uuid.to_string()) else {
                continue;
            };
            if thread_ids.contains(&thread_id) {
                found.entry(thread_id).or_insert_with(|| entry.path());
                if found.len() == thread_ids.len() {
                    return Ok(found);
                }
            }
        }
    }
    Ok(found)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn list_raw_threads(
    config: &impl RolloutConfigView,
    state_db_ctx: Option<&praxis_state::StateRuntime>,
    page_size: usize,
    cursor: Option<&Cursor>,
    sort_key: ThreadSortKey,
    allowed_sources: &[SessionSource],
    source_kinds: Option<&[praxis_state::ThreadSourceKind]>,
    model_providers: Option<&[String]>,
    default_provider: &str,
    archived: bool,
    cwd: Option<&Path>,
    search_term: Option<&str>,
) -> std::io::Result<ThreadsPage> {
    let praxis_home = config.praxis_home();
    if let Some(ctx) = state_db_ctx {
        let backfill_complete =
            state_db::is_backfill_complete(Some(ctx), "list_threads_with_db_fallback")
                .await
                .unwrap_or(false);
        if let Some(db_page) = state_db::list_threads_db(
            Some(ctx),
            praxis_home,
            page_size,
            cursor,
            sort_key,
            allowed_sources,
            source_kinds,
            model_providers,
            archived,
            cwd,
            search_term,
        )
        .await
        {
            let can_return_db_page = backfill_complete
                || cursor.is_some()
                || search_term.is_some()
                || db_page.items.len() >= page_size
                || db_page.next_anchor.is_some();
            if can_return_db_page {
                return Ok(db_page.into());
            }
            tracing::warn!(
                "state db returned a partial first page before backfill completed; falling back to session files"
            );
        } else if backfill_complete {
            tracing::warn!(
                "state db list failed after backfill completed; returning an empty partial page"
            );
            return Ok(ThreadsPage::default());
        } else if search_term.is_some() {
            tracing::warn!(
                "state db search failed before backfill completed; returning an empty partial page"
            );
            return Ok(ThreadsPage::default());
        } else {
            tracing::warn!(
                "state db list failed before backfill completed; falling back to session files"
            );
        }
    }

    if search_term.is_some() {
        tracing::warn!("state db unavailable for indexed thread search; returning an empty page");
        return Ok(ThreadsPage::default());
    }

    let fs_page = if archived {
        get_threads_in_root(
            praxis_home.join(ARCHIVED_SESSIONS_SUBDIR),
            page_size,
            cursor,
            sort_key,
            ThreadListConfig {
                allowed_sources,
                model_providers,
                default_provider,
                layout: ThreadListLayout::Flat,
            },
        )
        .await?
    } else {
        get_threads(
            praxis_home,
            page_size,
            cursor,
            sort_key,
            allowed_sources,
            model_providers,
            default_provider,
        )
        .await?
    };

    let fs_page = filter_fs_page_by_cwd(fs_page, cwd, default_provider).await;
    Ok(truncate_fs_page(fs_page, page_size, sort_key))
}

fn truncate_fs_page(
    mut page: ThreadsPage,
    page_size: usize,
    sort_key: ThreadSortKey,
) -> ThreadsPage {
    if page.items.len() <= page_size {
        return page;
    }
    page.items.truncate(page_size);
    page.next_cursor = page.items.last().and_then(|item| {
        let file_name = item.path.file_name()?.to_str()?;
        let (created_at, id) = parse_timestamp_uuid_from_filename(file_name)?;
        let cursor_token = match sort_key {
            ThreadSortKey::CreatedAt => format!("{}|{id}", created_at.format(&Rfc3339).ok()?),
            ThreadSortKey::UpdatedAt => format!("{}|{id}", item.updated_at.as_deref()?),
        };
        parse_cursor(cursor_token.as_str())
    });
    page
}

impl From<praxis_state::ThreadsPage> for ThreadsPage {
    fn from(db_page: praxis_state::ThreadsPage) -> Self {
        let items = db_page
            .items
            .into_iter()
            .map(|item| ThreadItem {
                path: item.rollout_path,
                thread_id: Some(item.id),
                first_user_message: item.first_user_message,
                cwd: Some(item.cwd),
                git_branch: item.git_branch,
                git_sha: item.git_sha,
                git_origin_url: item.git_origin_url,
                source: Some(
                    serde_json::from_str(item.source.as_str())
                        .or_else(|_| serde_json::from_value(Value::String(item.source)))
                        .unwrap_or(SessionSource::Unknown),
                ),
                agent_base_name: item.agent_base_name,
                agent_title: item.agent_title,
                agent_display_name: item.agent_display_name,
                agent_role: item.agent_role,
                model_provider: Some(item.model_provider),
                cli_version: Some(item.cli_version),
                created_at: Some(item.created_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
                updated_at: Some(item.updated_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
            })
            .collect();
        Self {
            items,
            next_cursor: db_page.next_anchor.map(Into::into),
            num_scanned_files: db_page.num_scanned_rows,
            reached_scan_cap: false,
        }
    }
}
