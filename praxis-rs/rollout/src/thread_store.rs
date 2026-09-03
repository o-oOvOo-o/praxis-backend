use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

use chrono::SecondsFormat;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::ResumedHistory;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::TokenUsageInfo;
use praxis_state::ThreadSourceKind;
use serde_json::Value;
use tracing::info;
use tracing::warn;

use crate::INTERACTIVE_SESSION_SOURCES;
use crate::RolloutConfigView;
use crate::list::Cursor;
use crate::list::ThreadArchiveFilter;
use crate::list::ThreadItem;
use crate::list::ThreadSortKey;
use crate::list::ThreadsPage;
use crate::metadata;
use crate::state_db;
use crate::state_db::StateDbHandle;

mod directory;
mod history;
mod migration;
mod names;
pub(crate) mod native_codec;
pub(crate) mod resume_selection;
mod rollout_stream;

pub use history::ThreadHistoryReader;
pub use names::ThreadNameResolver;
pub use names::ThreadNameWriter;
use resume_selection::select_resume_path;
pub(crate) use rollout_stream::RolloutItemStream;

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadGitInfo {
    pub sha: Option<String>,
    pub branch: Option<String>,
    pub origin_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadSummary {
    pub conversation_id: ThreadId,
    pub path: PathBuf,
    pub preview: String,
    pub summary: Option<String>,
    pub timestamp: Option<String>,
    pub updated_at: Option<String>,
    pub model_provider: String,
    pub model: Option<String>,
    pub cwd: PathBuf,
    pub cli_version: String,
    pub source: SessionSource,
    pub total_cost_micros: Option<i64>,
    pub last_cost_micros: Option<i64>,
    pub token_usage_info: Option<TokenUsageInfo>,
    pub selfwork_plan_path: Option<PathBuf>,
    pub git_info: Option<ThreadGitInfo>,
    pub thread_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadSummaryPage {
    pub items: Vec<ThreadSummary>,
    pub next_cursor: Option<Cursor>,
    pub num_scanned_files: usize,
    pub reached_scan_cap: bool,
}

#[derive(Debug, Clone)]
pub struct ListThreadsQuery {
    pub page_size: usize,
    pub cursor: Option<Cursor>,
    pub sort_key: ThreadSortKey,
    pub model_providers: Option<Vec<String>>,
    pub source_kinds: Option<Vec<ThreadSourceKind>>,
    pub archived: bool,
    pub cwd: Option<PathBuf>,
    pub search_term: Option<String>,
    pub fallback_provider: String,
}

/// Canonical owner for persisted thread discovery and metadata operations.
pub struct ThreadStore<'a, C: RolloutConfigView> {
    config: &'a C,
    state_db: Option<StateDbHandle>,
    native_store: praxis_thread_store::ThreadStore,
    history: ThreadHistoryReader,
}

struct SourceFilters {
    allowed_sources: Vec<SessionSource>,
    source_kinds: Option<Vec<ThreadSourceKind>>,
    post_filter: Option<Vec<ThreadSourceKind>>,
}

enum DirectoryPage {
    Native(Vec<ThreadSummary>, Option<Cursor>),
    Db(praxis_state::ThreadsPage),
    Fs(ThreadsPage),
}

impl<'a, C: RolloutConfigView> ThreadStore<'a, C> {
    pub async fn open(config: &'a C) -> Self {
        let state_db = state_db::get_state_db(config).await;
        migration::ensure_started(config);
        let native_store =
            praxis_thread_store::ThreadStore::from_praxis_home(config.praxis_home().to_path_buf());
        let history = ThreadHistoryReader::from_native_store(native_store.clone());
        Self {
            config,
            state_db,
            native_store,
            history,
        }
    }

    pub fn state_db(&self) -> Option<&praxis_state::StateRuntime> {
        self.state_db.as_deref()
    }

    pub fn history(&self) -> &ThreadHistoryReader {
        &self.history
    }

    pub async fn fold_rollout_items<S, F>(
        &self,
        rollout_path: &Path,
        state: S,
        fold: F,
    ) -> io::Result<S>
    where
        S: Send + 'static,
        F: FnMut(&mut S, RolloutItem) + Send + 'static,
    {
        self.history.fold_items(rollout_path, state, fold).await
    }

    pub async fn read_rollout_items(&self, rollout_path: &Path) -> io::Result<Vec<RolloutItem>> {
        self.history.read_items(rollout_path).await
    }

    pub async fn read_initial_history(&self, rollout_path: &Path) -> io::Result<InitialHistory> {
        self.history.read_initial_history(rollout_path).await
    }

    pub fn name_resolver(&self) -> ThreadNameResolver<'_> {
        ThreadNameResolver::new(self.state_db())
    }

    pub fn name_writer(&self) -> ThreadNameWriter<'_> {
        ThreadNameWriter::with_praxis_home(self.state_db(), self.config.praxis_home())
    }

    pub async fn list_threads(&self, query: ListThreadsQuery) -> io::Result<ThreadSummaryPage> {
        list_threads_with_state_db(self.config, self.state_db(), &self.native_store, query).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_raw_threads(
        &self,
        page_size: usize,
        cursor: Option<&Cursor>,
        sort_key: ThreadSortKey,
        allowed_sources: &[SessionSource],
        model_providers: Option<&[String]>,
        default_provider: &str,
        cwd: Option<&Path>,
        search_term: Option<&str>,
    ) -> io::Result<ThreadsPage> {
        directory::list_raw_threads(
            self.config,
            self.state_db(),
            page_size,
            cursor,
            sort_key,
            allowed_sources,
            None,
            model_providers,
            default_provider,
            false,
            cwd,
            search_term,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_archived_raw_threads(
        &self,
        page_size: usize,
        cursor: Option<&Cursor>,
        sort_key: ThreadSortKey,
        allowed_sources: &[SessionSource],
        model_providers: Option<&[String]>,
        default_provider: &str,
        cwd: Option<&Path>,
        search_term: Option<&str>,
    ) -> io::Result<ThreadsPage> {
        directory::list_raw_threads(
            self.config,
            self.state_db(),
            page_size,
            cursor,
            sort_key,
            allowed_sources,
            None,
            model_providers,
            default_provider,
            true,
            cwd,
            search_term,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn find_latest_rollout_path(
        &self,
        page_size: usize,
        cursor: Option<&Cursor>,
        sort_key: ThreadSortKey,
        allowed_sources: &[SessionSource],
        model_providers: Option<&[String]>,
        default_provider: &str,
        filter_cwd: Option<&Path>,
    ) -> io::Result<Option<PathBuf>> {
        let mut cursor = cursor.cloned();
        loop {
            let page = self
                .list_raw_threads(
                    page_size,
                    cursor.as_ref(),
                    sort_key,
                    allowed_sources,
                    model_providers,
                    default_provider,
                    filter_cwd,
                    None,
                )
                .await?;
            if let Some(path) = select_resume_path(&page, filter_cwd, default_provider).await {
                return Ok(Some(path));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(None);
            }
        }
    }

    pub async fn resolve_thread_names(
        &self,
        thread_ids: &HashSet<ThreadId>,
    ) -> HashMap<ThreadId, String> {
        self.name_resolver().resolve_names(thread_ids).await
    }

    pub async fn resolve_thread_name(&self, thread_id: ThreadId) -> Option<String> {
        self.name_resolver().resolve_name(thread_id).await
    }

    pub async fn write_thread_name(&self, thread_id: ThreadId, name: &str) -> io::Result<()> {
        self.name_writer().write_name(thread_id, name).await
    }

    pub async fn set_archived(&self, thread_id: ThreadId, archived: bool) -> io::Result<()> {
        let native_id =
            praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
                .map_err(|error| io::Error::other(error.to_string()))?;
        if self.native_store.thread_exists(native_id).await {
            self.native_store
                .set_archived(native_id, archived)
                .await
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        Ok(())
    }

    pub async fn delete_native_thread(&self, thread_id: ThreadId) -> io::Result<bool> {
        let native_id =
            praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
                .map_err(|error| io::Error::other(error.to_string()))?;
        self.native_store
            .delete_thread(native_id)
            .await
            .map_err(|error| io::Error::other(error.to_string()))
    }

    pub async fn thread_exists(
        &self,
        thread_id: ThreadId,
        archived_only: Option<bool>,
    ) -> io::Result<bool> {
        Ok(self
            .find_rollout_path(thread_id, archived_only)
            .await?
            .is_some())
    }

    pub async fn find_rollout_path(
        &self,
        thread_id: ThreadId,
        archived_only: Option<bool>,
    ) -> io::Result<Option<PathBuf>> {
        crate::list::find_thread_path_by_id_str_with_db_context(
            self.config.praxis_home(),
            &thread_id.to_string(),
            ThreadArchiveFilter::from_archived_only(archived_only),
            self.state_db(),
        )
        .await
    }

    pub async fn read_thread_summary(
        &self,
        thread_id: ThreadId,
        archived_only: Option<bool>,
        fallback_provider: &str,
    ) -> io::Result<Option<ThreadSummary>> {
        if let Some(metadata) = self.read_thread_metadata_from_db(thread_id).await {
            if !thread_archive_matches(&metadata, archived_only) {
                return Ok(None);
            }
            if tokio::fs::try_exists(&metadata.rollout_path)
                .await
                .unwrap_or(false)
            {
                let mut summary = summary_from_thread_metadata(&metadata);
                self.attach_thread_name(&mut summary).await;
                return Ok(Some(summary));
            }
            warn!(
                "state db returned stale rollout path for thread {}: {}",
                thread_id,
                metadata.rollout_path.display()
            );
        }

        let Some(rollout_path) = self.find_rollout_path(thread_id, archived_only).await? else {
            return Ok(None);
        };
        let outcome = metadata::extract_metadata_from_rollout(&rollout_path, fallback_provider)
            .await
            .map_err(|err| io::Error::other(err.to_string()))?;
        let mut metadata = outcome.metadata;
        if metadata.id != thread_id {
            warn!(
                "rollout path {} resolved for thread {} but contains metadata for {}",
                rollout_path.display(),
                thread_id,
                metadata.id
            );
            return Ok(None);
        }
        metadata.cwd = state_db::normalize_cwd_for_state_db(&metadata.cwd);
        if matches!(archived_only, Some(true)) && metadata.archived_at.is_none() {
            metadata.archived_at = Some(metadata.updated_at);
        }
        if !thread_archive_matches(&metadata, archived_only) {
            return Ok(None);
        }
        state_db::read_repair_rollout_path(
            self.state_db(),
            Some(thread_id),
            archived_only,
            rollout_path.as_path(),
        )
        .await;

        let mut summary = summary_from_thread_metadata(&metadata);
        self.attach_thread_name(&mut summary).await;
        Ok(Some(summary))
    }

    pub async fn read_history_cwd(
        &self,
        thread_id: Option<ThreadId>,
        rollout_path: &Path,
    ) -> Option<PathBuf> {
        if let Some(state_db_ctx) = self.state_db()
            && let Some(thread_id) = thread_id
            && let Ok(Some(metadata)) = state_db_ctx.get_thread(thread_id).await
            && !metadata.cwd.as_os_str().is_empty()
        {
            return Some(metadata.cwd);
        }

        match crate::list::read_session_meta_line(rollout_path).await {
            Ok(meta_line) => Some(meta_line.meta.cwd),
            Err(err) => {
                warn!(
                    "failed to read session metadata from rollout {}: {err}",
                    rollout_path.display()
                );
                None
            }
        }
    }

    async fn read_thread_metadata_from_db(
        &self,
        thread_id: ThreadId,
    ) -> Option<praxis_state::ThreadMetadata> {
        let state_db = self.state_db()?;
        match state_db.get_thread(thread_id).await {
            Ok(metadata) => metadata,
            Err(err) => {
                warn!("state db get_thread failed for {thread_id}: {err}");
                None
            }
        }
    }

    async fn attach_thread_name(&self, summary: &mut ThreadSummary) {
        summary.thread_name = self.resolve_thread_name(summary.conversation_id).await;
    }
}

/// Reads all persisted items and the canonical thread identity in one scan.
pub async fn read_items(path: &Path) -> io::Result<(Vec<RolloutItem>, Option<ThreadId>, usize)> {
    let mut items = Vec::new();
    let (thread_id, parse_errors) = scan_items(path, |item| items.push(item)).await?;
    Ok((items, thread_id, parse_errors))
}

/// Streams persisted items through a caller-owned projection without retaining the transcript.
pub async fn scan_items(
    path: &Path,
    mut on_item: impl FnMut(RolloutItem),
) -> io::Result<(Option<ThreadId>, usize)> {
    let mut stream = RolloutItemStream::open(path).await?;
    while let Some(item) = stream.next_item().await? {
        on_item(item);
    }
    let (thread_id, parse_errors) = stream.outcome();
    tracing::debug!(
        item_count = stream.item_count(),
        ?thread_id,
        parse_errors,
        "scanned persisted thread"
    );
    Ok((thread_id, parse_errors))
}

/// Hydrates the canonical initial-history value for a persisted thread.
pub async fn read_initial_history(path: &Path) -> io::Result<InitialHistory> {
    let (items, thread_id, _) = read_items(path).await?;
    let conversation_id =
        thread_id.ok_or_else(|| IoError::other("failed to parse thread ID from session file"))?;
    if items.is_empty() {
        return Ok(InitialHistory::New);
    }
    info!("Hydrated persisted thread from {path:?}");
    Ok(InitialHistory::Resumed(ResumedHistory {
        conversation_id,
        history: items,
        rollout_path: path.to_path_buf(),
    }))
}

async fn list_threads_with_state_db(
    config: &impl RolloutConfigView,
    state_db_ctx: Option<&praxis_state::StateRuntime>,
    native_store: &praxis_thread_store::ThreadStore,
    query: ListThreadsQuery,
) -> io::Result<ThreadSummaryPage> {
    let source_filters = compute_source_filters(query.source_kinds);

    let mut cursor = query.cursor;
    let mut last_cursor = cursor.clone();
    let mut remaining = query.page_size;
    let mut items = Vec::with_capacity(query.page_size);
    let mut next_cursor = None;
    let mut num_scanned_files = 0usize;
    let mut reached_scan_cap = false;

    while remaining > 0 {
        let page_size = remaining;
        let page = list_directory_page(
            config,
            state_db_ctx,
            native_store,
            page_size,
            cursor.as_ref(),
            query.sort_key,
            &source_filters,
            query.model_providers.as_deref(),
            query.fallback_provider.as_str(),
            query.archived,
            query.cwd.as_deref(),
            query.search_term.as_deref(),
        )
        .await?;

        let (mut summaries, page_next_cursor, page_scanned_files, page_reached_scan_cap) =
            match page {
                DirectoryPage::Native(summaries, next_cursor) => {
                    let num_scanned_rows = summaries.len();
                    (summaries, next_cursor, num_scanned_rows, false)
                }
                DirectoryPage::Db(page) => {
                    let next_cursor = page.next_anchor.map(Into::into);
                    let num_scanned_rows = page.num_scanned_rows;
                    let summaries = page
                        .items
                        .into_iter()
                        .map(|metadata| summary_from_thread_metadata(&metadata))
                        .collect::<Vec<_>>();
                    (summaries, next_cursor, num_scanned_rows, false)
                }
                DirectoryPage::Fs(page) => {
                    let next_cursor = page.next_cursor;
                    let num_scanned_files = page.num_scanned_files;
                    let reached_scan_cap = page.reached_scan_cap;
                    let summaries =
                        summarize_thread_items(page.items, state_db_ctx, &query.fallback_provider)
                            .await;
                    (summaries, next_cursor, num_scanned_files, reached_scan_cap)
                }
            };
        num_scanned_files = num_scanned_files.saturating_add(page_scanned_files);
        reached_scan_cap |= page_reached_scan_cap;
        if let Some(filter) = source_filters.post_filter.as_ref() {
            summaries.retain(|summary| source_kind_matches(&summary.source, filter));
        }

        if summaries.len() > remaining {
            summaries.truncate(remaining);
        }
        items.extend(summaries);
        remaining = query.page_size.saturating_sub(items.len());

        next_cursor = page_next_cursor;
        if remaining == 0 {
            break;
        }

        match next_cursor.clone() {
            Some(cursor_value) if remaining > 0 => {
                if last_cursor.as_ref() == Some(&cursor_value) {
                    next_cursor = None;
                    break;
                }
                last_cursor = Some(cursor_value.clone());
                cursor = Some(cursor_value);
            }
            _ => break,
        }
    }

    let thread_ids = items
        .iter()
        .map(|summary| summary.conversation_id)
        .collect::<HashSet<_>>();
    let names = ThreadNameResolver::new(state_db_ctx)
        .resolve_names(&thread_ids)
        .await;
    for summary in &mut items {
        if let Some(name) = names.get(&summary.conversation_id) {
            summary.thread_name = Some(name.clone());
        }
    }

    Ok(ThreadSummaryPage {
        items,
        next_cursor,
        num_scanned_files,
        reached_scan_cap,
    })
}

#[allow(clippy::too_many_arguments)]
async fn list_directory_page(
    config: &impl RolloutConfigView,
    state_db_ctx: Option<&praxis_state::StateRuntime>,
    native_store: &praxis_thread_store::ThreadStore,
    page_size: usize,
    cursor: Option<&Cursor>,
    sort_key: ThreadSortKey,
    source_filters: &SourceFilters,
    model_providers: Option<&[String]>,
    fallback_provider: &str,
    archived: bool,
    cwd: Option<&Path>,
    search_term: Option<&str>,
) -> std::io::Result<DirectoryPage> {
    if migration::is_complete(config.praxis_home()) {
        match list_native_directory_page(
            config,
            state_db_ctx,
            native_store,
            page_size,
            cursor,
            sort_key,
            source_filters,
            model_providers,
            fallback_provider,
            archived,
            cwd,
            search_term,
        )
        .await
        {
            Ok(page) => return Ok(page),
            Err(error) => {
                warn!("native thread index list failed; using migration fallback: {error}")
            }
        }
    }

    if let Some(ctx) = state_db_ctx {
        let backfill_complete = state_db::is_backfill_complete(Some(ctx), "list_threads")
            .await
            .unwrap_or(false);
        if let Some(db_page) = state_db::list_threads_db(
            Some(ctx),
            config.praxis_home(),
            page_size,
            cursor,
            sort_key,
            source_filters.allowed_sources.as_slice(),
            source_filters.source_kinds.as_deref(),
            model_providers,
            archived,
            cwd,
            search_term,
        )
        .await
        {
            if should_return_db_page(&db_page, page_size, cursor, search_term, backfill_complete) {
                return Ok(DirectoryPage::Db(db_page));
            }
            warn!(
                "state db returned a partial first directory page before backfill completed; falling back to session files"
            );
        } else if backfill_complete {
            warn!("state db directory list failed after backfill completed; returning empty page");
            return Ok(DirectoryPage::Fs(ThreadsPage::default()));
        } else if search_term.is_some() {
            warn!(
                "state db directory search failed before backfill completed; returning empty page"
            );
            return Ok(DirectoryPage::Fs(ThreadsPage::default()));
        } else {
            warn!(
                "state db directory list failed before backfill completed; falling back to session files"
            );
        }
    }

    let page = directory::list_raw_threads(
        config,
        None,
        page_size,
        cursor,
        sort_key,
        source_filters.allowed_sources.as_slice(),
        source_filters.source_kinds.as_deref(),
        model_providers,
        fallback_provider,
        archived,
        cwd,
        search_term,
    )
    .await?;
    Ok(DirectoryPage::Fs(page))
}

#[allow(clippy::too_many_arguments)]
async fn list_native_directory_page(
    config: &impl RolloutConfigView,
    state_db_ctx: Option<&praxis_state::StateRuntime>,
    native_store: &praxis_thread_store::ThreadStore,
    page_size: usize,
    cursor: Option<&Cursor>,
    sort_key: ThreadSortKey,
    source_filters: &SourceFilters,
    model_providers: Option<&[String]>,
    fallback_provider: &str,
    archived: bool,
    cwd: Option<&Path>,
    search_term: Option<&str>,
) -> io::Result<DirectoryPage> {
    let mut query = praxis_thread_store::ThreadListQuery {
        archived: Some(archived),
        workspace: cwd.map(|path| {
            state_db::normalize_cwd_for_state_db(path)
                .to_string_lossy()
                .into_owned()
        }),
        sources: Some(
            source_filters
                .allowed_sources
                .iter()
                .map(ToString::to_string)
                .collect(),
        ),
        model_providers: model_providers.map(<[String]>::to_vec),
        search: search_term.map(str::to_owned),
        limit: Some(page_size),
        sort: match sort_key {
            ThreadSortKey::CreatedAt => praxis_thread_store::ThreadListSort::CreatedAt,
            ThreadSortKey::UpdatedAt => praxis_thread_store::ThreadListSort::UpdatedAt,
        },
        ..praxis_thread_store::ThreadListQuery::default()
    };
    if let Some(cursor) = cursor {
        let (sort_value, id) = cursor.native_parts();
        let thread_id = praxis_thread_store_contracts::ThreadId::parse(id.to_string().as_str())
            .map_err(|error| io::Error::other(error.to_string()))?;
        query.set_cursor_after(sort_value, thread_id);
    }
    let page = native_store
        .list_threads(query)
        .await
        .map_err(|error| io::Error::other(error.to_string()))?;
    let protocol_ids = page
        .items
        .iter()
        .filter_map(|summary| ThreadId::from_string(&summary.thread_id.to_string()).ok())
        .collect::<HashSet<_>>();
    let state_threads = if let Some(state_db) = state_db_ctx {
        state_db
            .get_threads(&protocol_ids)
            .await
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    let missing_locators = protocol_ids
        .iter()
        .filter(|thread_id| !state_threads.contains_key(thread_id))
        .copied()
        .collect::<HashSet<_>>();
    let locator_root = config.praxis_home().join(if archived {
        crate::ARCHIVED_SESSIONS_SUBDIR
    } else {
        crate::SESSIONS_SUBDIR
    });
    let fallback_paths = directory::find_rollout_paths(&locator_root, &missing_locators).await?;
    for (thread_id, path) in &fallback_paths {
        state_db::read_repair_rollout_path(state_db_ctx, Some(*thread_id), Some(archived), path)
            .await;
    }
    let next_cursor = page.next_cursor.as_ref().and_then(|_| {
        let last = page.items.last()?;
        let unix_ms = match sort_key {
            ThreadSortKey::CreatedAt => last.created_at_unix_ms,
            ThreadSortKey::UpdatedAt => last.updated_at_unix_ms,
        };
        let id = uuid::Uuid::parse_str(&last.thread_id.to_string()).ok()?;
        Cursor::from_native_parts(unix_ms, id)
    });
    let mut summaries = Vec::with_capacity(page.items.len());
    for native in page.items {
        let Some(thread_id) = ThreadId::from_string(&native.thread_id.to_string()).ok() else {
            continue;
        };
        let path = if let Some(metadata) = state_threads.get(&thread_id) {
            Some(metadata.rollout_path.clone())
        } else {
            fallback_paths.get(&thread_id).cloned()
        };
        let Some(path) = path else {
            continue;
        };
        let mut summary = state_threads
            .get(&thread_id)
            .map(summary_from_thread_metadata)
            .unwrap_or_else(|| summary_from_native(native.clone(), thread_id, path));
        summary.preview = native.first_user_message.unwrap_or_default();
        summary.summary = native.summary;
        summary.model_provider = native
            .model_provider
            .unwrap_or_else(|| fallback_provider.to_owned());
        summary.model = native.model;
        summary.cwd = PathBuf::from(native.workspace);
        summary.total_cost_micros = native.total_cost_micros;
        summary.last_cost_micros = native.last_cost_micros;
        summary.thread_name = native.name;
        summaries.push(summary);
    }
    Ok(DirectoryPage::Native(summaries, next_cursor))
}

fn summary_from_native(
    native: praxis_thread_store::ThreadSummary,
    thread_id: ThreadId,
    path: PathBuf,
) -> ThreadSummary {
    let timestamp = chrono::DateTime::from_timestamp_millis(native.created_at_unix_ms)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true));
    let updated_at = chrono::DateTime::from_timestamp_millis(native.updated_at_unix_ms)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true));
    ThreadSummary {
        conversation_id: thread_id,
        path,
        preview: native.first_user_message.unwrap_or_default(),
        summary: native.summary,
        timestamp,
        updated_at,
        model_provider: native.model_provider.unwrap_or_default(),
        model: native.model,
        cwd: PathBuf::from(native.workspace),
        cli_version: String::new(),
        source: parse_session_source(native.source.as_str()),
        total_cost_micros: native.total_cost_micros,
        last_cost_micros: native.last_cost_micros,
        token_usage_info: None,
        selfwork_plan_path: None,
        git_info: None,
        thread_name: native.name,
    }
}

fn should_return_db_page(
    db_page: &praxis_state::ThreadsPage,
    page_size: usize,
    cursor: Option<&Cursor>,
    search_term: Option<&str>,
    backfill_complete: bool,
) -> bool {
    backfill_complete
        || cursor.is_some()
        || search_term.is_some()
        || db_page.items.len() >= page_size
        || db_page.next_anchor.is_some()
}

fn thread_archive_matches(
    metadata: &praxis_state::ThreadMetadata,
    archived_only: Option<bool>,
) -> bool {
    match archived_only {
        Some(true) => metadata.archived_at.is_some(),
        Some(false) => metadata.archived_at.is_none(),
        None => true,
    }
}

fn compute_source_filters(source_kinds: Option<Vec<ThreadSourceKind>>) -> SourceFilters {
    let Some(source_kinds) = source_kinds else {
        return SourceFilters {
            allowed_sources: INTERACTIVE_SESSION_SOURCES.to_vec(),
            source_kinds: None,
            post_filter: None,
        };
    };

    if source_kinds.is_empty() {
        return SourceFilters {
            allowed_sources: INTERACTIVE_SESSION_SOURCES.to_vec(),
            source_kinds: None,
            post_filter: None,
        };
    }

    SourceFilters {
        allowed_sources: Vec::new(),
        source_kinds: Some(source_kinds.clone()),
        post_filter: Some(source_kinds),
    }
}

fn source_kind_matches(source: &SessionSource, filter: &[ThreadSourceKind]) -> bool {
    filter.iter().any(|kind| match kind {
        ThreadSourceKind::Cli => matches!(source, SessionSource::Cli),
        ThreadSourceKind::VsCode => matches!(source, SessionSource::VSCode),
        ThreadSourceKind::Exec => matches!(source, SessionSource::Exec),
        ThreadSourceKind::AppGateway => matches!(source, SessionSource::AppGateway),
        ThreadSourceKind::SubAgent => matches!(source, SessionSource::SubAgent(_)),
        ThreadSourceKind::SubAgentReview => {
            matches!(source, SessionSource::SubAgent(SubAgentSource::Review))
        }
        ThreadSourceKind::SubAgentCompact => {
            matches!(source, SessionSource::SubAgent(SubAgentSource::Compact))
        }
        ThreadSourceKind::SubAgentThreadSpawn => matches!(
            source,
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. })
        ),
        ThreadSourceKind::SubAgentOther => {
            matches!(source, SessionSource::SubAgent(SubAgentSource::Other(_)))
        }
        ThreadSourceKind::Unknown => matches!(source, SessionSource::Unknown),
    })
}

async fn summarize_thread_items(
    items: Vec<ThreadItem>,
    state_db: Option<&praxis_state::StateRuntime>,
    fallback_provider: &str,
) -> Vec<ThreadSummary> {
    let thread_ids = items
        .iter()
        .filter_map(|item| {
            item.thread_id
                .or_else(|| thread_id_from_rollout_path(&item.path))
        })
        .collect::<HashSet<_>>();
    let state_threads = match state_db {
        Some(state_db) if !thread_ids.is_empty() => match state_db.get_threads(&thread_ids).await {
            Ok(threads) => threads,
            Err(err) => {
                warn!("Failed to read batched thread metadata from state db: {err}");
                HashMap::new()
            }
        },
        _ => HashMap::new(),
    };

    items
        .into_iter()
        .filter_map(|item| {
            let thread_id = item
                .thread_id
                .or_else(|| thread_id_from_rollout_path(&item.path))?;
            if let Some(metadata) = state_threads.get(&thread_id) {
                return Some(summary_from_thread_metadata(metadata));
            }
            summary_from_thread_item(item, thread_id, fallback_provider)
        })
        .collect()
}

fn summary_from_thread_item(
    item: ThreadItem,
    thread_id: ThreadId,
    fallback_provider: &str,
) -> Option<ThreadSummary> {
    let timestamp = item.created_at.clone();
    let updated_at = item.updated_at.clone().or_else(|| timestamp.clone());
    let cwd = item.cwd?;
    let source = with_thread_spawn_agent_metadata(
        item.source.unwrap_or(SessionSource::Unknown),
        item.agent_base_name,
        item.agent_title,
        item.agent_display_name,
        item.agent_role,
    );
    Some(ThreadSummary {
        conversation_id: thread_id,
        path: item.path,
        preview: item.first_user_message.unwrap_or_default(),
        summary: None,
        timestamp,
        updated_at,
        model_provider: item
            .model_provider
            .unwrap_or_else(|| fallback_provider.to_string()),
        model: None,
        cwd,
        cli_version: item.cli_version.unwrap_or_default(),
        source,
        total_cost_micros: None,
        last_cost_micros: None,
        token_usage_info: None,
        selfwork_plan_path: None,
        git_info: thread_git_info(item.git_sha, item.git_branch, item.git_origin_url),
        thread_name: None,
    })
}

fn summary_from_thread_metadata(metadata: &praxis_state::ThreadMetadata) -> ThreadSummary {
    ThreadSummary {
        conversation_id: metadata.id,
        path: metadata.rollout_path.clone(),
        preview: metadata.first_user_message.clone().unwrap_or_default(),
        summary: metadata.session_summary.clone(),
        timestamp: Some(
            metadata
                .created_at
                .to_rfc3339_opts(SecondsFormat::Secs, true),
        ),
        updated_at: Some(
            metadata
                .updated_at
                .to_rfc3339_opts(SecondsFormat::Secs, true),
        ),
        model_provider: metadata.model_provider.clone(),
        model: metadata.model.clone(),
        cwd: metadata.cwd.clone(),
        cli_version: metadata.cli_version.clone(),
        source: with_thread_spawn_agent_metadata(
            parse_session_source(metadata.source.as_str()),
            metadata.agent_base_name.clone(),
            metadata.agent_title.clone(),
            metadata.agent_display_name.clone(),
            metadata.agent_role.clone(),
        ),
        total_cost_micros: metadata.total_cost_micros,
        last_cost_micros: metadata.last_cost_micros,
        token_usage_info: metadata.token_usage_info.clone(),
        selfwork_plan_path: metadata.selfwork_plan_path.clone(),
        git_info: thread_git_info(
            metadata.git_sha.clone(),
            metadata.git_branch.clone(),
            metadata.git_origin_url.clone(),
        ),
        thread_name: None,
    }
}

fn parse_session_source(source: &str) -> SessionSource {
    serde_json::from_str(source)
        .or_else(|_| serde_json::from_value(Value::String(source.to_string())))
        .unwrap_or(SessionSource::Unknown)
}

fn with_thread_spawn_agent_metadata(
    source: SessionSource,
    agent_base_name: Option<String>,
    agent_title: Option<String>,
    agent_display_name: Option<String>,
    agent_role: Option<String>,
) -> SessionSource {
    if agent_base_name.is_none()
        && agent_title.is_none()
        && agent_display_name.is_none()
        && agent_role.is_none()
    {
        return source;
    }

    match source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path,
            agent_base_name: existing_agent_base_name,
            agent_title: existing_agent_title,
            agent_display_name: existing_agent_display_name,
            agent_role: existing_agent_role,
        }) => SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path,
            agent_base_name: agent_base_name.or(existing_agent_base_name),
            agent_title: agent_title.or(existing_agent_title),
            agent_display_name: agent_display_name.or(existing_agent_display_name),
            agent_role: agent_role.or(existing_agent_role),
        }),
        _ => source,
    }
}

fn thread_git_info(
    sha: Option<String>,
    branch: Option<String>,
    origin_url: Option<String>,
) -> Option<ThreadGitInfo> {
    if sha.is_none() && branch.is_none() && origin_url.is_none() {
        None
    } else {
        Some(ThreadGitInfo {
            sha,
            branch,
            origin_url,
        })
    }
}

pub(crate) fn thread_id_from_rollout_path(path: &Path) -> Option<ThreadId> {
    let file_name = path.file_name()?.to_str()?;
    let stem = file_name.strip_suffix(".jsonl")?;
    if stem.len() < 37 {
        return None;
    }
    let uuid_start = stem.len().saturating_sub(36);
    if !stem[..uuid_start].ends_with('-') {
        return None;
    }
    ThreadId::from_string(&stem[uuid_start..]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_filters_default_to_interactive_sources() {
        let filters = compute_source_filters(None);

        assert_eq!(
            filters.allowed_sources,
            INTERACTIVE_SESSION_SOURCES.to_vec()
        );
        assert_eq!(filters.source_kinds, None);
        assert_eq!(filters.post_filter, None);
    }

    #[test]
    fn source_filters_explicit_sources_use_indexed_kinds() {
        let source_kinds = vec![
            ThreadSourceKind::Cli,
            ThreadSourceKind::VsCode,
            ThreadSourceKind::Exec,
            ThreadSourceKind::AppGateway,
            ThreadSourceKind::SubAgentReview,
            ThreadSourceKind::SubAgentCompact,
            ThreadSourceKind::Unknown,
        ];
        let filters = compute_source_filters(Some(source_kinds.clone()));

        assert_eq!(filters.allowed_sources, Vec::<SessionSource>::new());
        assert_eq!(filters.source_kinds, Some(source_kinds.clone()));
        assert_eq!(filters.post_filter, Some(source_kinds));
    }

    #[test]
    fn source_filters_open_subagent_sources_require_post_filtering() {
        let source_kinds = vec![ThreadSourceKind::SubAgentThreadSpawn];
        let filters = compute_source_filters(Some(source_kinds.clone()));

        assert_eq!(filters.allowed_sources, Vec::<SessionSource>::new());
        assert_eq!(filters.source_kinds, Some(source_kinds.clone()));
        assert_eq!(filters.post_filter, Some(source_kinds));
    }

    #[test]
    fn partial_first_db_page_is_not_trusted_before_backfill() {
        let empty_page = praxis_state::ThreadsPage {
            items: Vec::new(),
            next_anchor: None,
            num_scanned_rows: 0,
        };

        assert!(!should_return_db_page(&empty_page, 25, None, None, false));
        assert!(should_return_db_page(
            &empty_page,
            25,
            None,
            Some("named thread"),
            false
        ));
        assert!(should_return_db_page(&empty_page, 25, None, None, true));
    }

    #[test]
    fn source_kind_match_distinguishes_subagent_variants() {
        let parent_thread_id = ThreadId::new();
        let review = SessionSource::SubAgent(SubAgentSource::Review);
        let spawn = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth: 1,
            agent_path: None,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
        });

        assert!(source_kind_matches(
            &review,
            &[ThreadSourceKind::SubAgentReview]
        ));
        assert!(!source_kind_matches(
            &review,
            &[ThreadSourceKind::SubAgentThreadSpawn]
        ));
        assert!(source_kind_matches(
            &spawn,
            &[ThreadSourceKind::SubAgentThreadSpawn]
        ));
        assert!(!source_kind_matches(
            &spawn,
            &[ThreadSourceKind::SubAgentReview]
        ));
    }
}
