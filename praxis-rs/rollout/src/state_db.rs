use crate::config::RolloutConfig;
use crate::config::RolloutConfigView;
use crate::list::Cursor;
use crate::list::ThreadSortKey;
use crate::metadata;
use chrono::DateTime;
use chrono::NaiveDateTime;
use chrono::Timelike;
use chrono::Utc;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionSource;
pub use praxis_state::LogEntry;
use praxis_state::ThreadMetadataBuilder;
use praxis_utils_path::normalize_for_path_comparison;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::MutexGuard as StdMutexGuard;
use std::sync::OnceLock;
use std::sync::RwLock;
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;
use uuid::Uuid;

/// Core-facing handle to the SQLite-backed state runtime.
pub type StateDbHandle = Arc<praxis_state::StateRuntime>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct StateDbKey {
    sqlite_home: PathBuf,
    model_provider_id: String,
}

struct StateDbCacheEntry {
    runtime: OnceLock<StateDbHandle>,
    init_lock: AsyncMutex<()>,
}

impl StateDbCacheEntry {
    fn new() -> Self {
        Self {
            runtime: OnceLock::new(),
            init_lock: AsyncMutex::new(()),
        }
    }
}

fn runtime_cache() -> &'static RwLock<HashMap<StateDbKey, Arc<StateDbCacheEntry>>> {
    static CACHE: OnceLock<RwLock<HashMap<StateDbKey, Arc<StateDbCacheEntry>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn backfill_checked_homes() -> &'static StdMutex<HashSet<PathBuf>> {
    static CHECKED: OnceLock<StdMutex<HashSet<PathBuf>>> = OnceLock::new();
    CHECKED.get_or_init(|| StdMutex::new(HashSet::new()))
}

fn lock_no_poison<T>(mutex: &StdMutex<T>) -> StdMutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn runtime_cache_entry(key: StateDbKey) -> Arc<StateDbCacheEntry> {
    if let Some(entry) = {
        let cache = match runtime_cache().read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        cache.get(&key).cloned()
    } {
        return entry;
    }

    let mut cache = match runtime_cache().write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    cache
        .entry(key)
        .or_insert_with(|| Arc::new(StateDbCacheEntry::new()))
        .clone()
}

/// Initialize the state runtime for thread state persistence and backfill checks.
pub async fn init(config: &impl RolloutConfigView) -> Option<StateDbHandle> {
    get_state_db(config).await
}

/// Get the process-cached state runtime without starting rollout metadata backfill.
pub async fn get_state_runtime(config: &impl RolloutConfigView) -> Option<StateDbHandle> {
    let config = RolloutConfig::from_view(config);
    get_or_init_runtime(&config).await
}

/// Get the process-cached state DB, creating it when needed.
pub async fn get_state_db(config: &impl RolloutConfigView) -> Option<StateDbHandle> {
    let config = RolloutConfig::from_view(config);
    let runtime = get_or_init_runtime(&config).await?;
    ensure_backfill_started(runtime.clone(), config).await;
    Some(runtime)
}

/// Return whether rollout metadata backfill is complete for this runtime.
pub async fn is_backfill_complete(
    context: Option<&praxis_state::StateRuntime>,
    stage: &str,
) -> Option<bool> {
    let ctx = context?;
    match ctx.get_backfill_state().await {
        Ok(state) => Some(state.status == praxis_state::BackfillStatus::Complete),
        Err(err) => {
            warn!("state db get_backfill_state failed during {stage}: {err}");
            None
        }
    }
}

/// Open the state runtime when the SQLite file exists, without feature gating.
///
/// This is used for parity checks during the SQLite migration phase.
pub async fn open_if_present(praxis_home: &Path, default_provider: &str) -> Option<StateDbHandle> {
    let db_path = praxis_state::state_db_path(praxis_home);
    if !tokio::fs::try_exists(&db_path).await.unwrap_or(false) {
        return None;
    }
    let config = RolloutConfig {
        praxis_home: praxis_home.to_path_buf(),
        sqlite_home: praxis_home.to_path_buf(),
        cwd: praxis_home.to_path_buf(),
        model_provider_id: default_provider.to_string(),
        generate_memories: false,
    };
    get_or_init_runtime(&config).await
}

async fn get_or_init_runtime(config: &RolloutConfig) -> Option<StateDbHandle> {
    let key = StateDbKey {
        sqlite_home: config.sqlite_home.clone(),
        model_provider_id: config.model_provider_id.clone(),
    };
    let entry = runtime_cache_entry(key);

    if let Some(runtime) = entry.runtime.get() {
        return Some(runtime.clone());
    }

    let _init_guard = entry.init_lock.lock().await;
    if let Some(runtime) = entry.runtime.get() {
        return Some(runtime.clone());
    }

    let runtime = match praxis_state::StateRuntime::init(
        config.sqlite_home.clone(),
        config.model_provider_id.clone(),
    )
    .await
    {
        Ok(runtime) => runtime,
        Err(err) => {
            warn!(
                "failed to initialize state runtime at {}: {err}",
                config.sqlite_home.display()
            );
            return None;
        }
    };
    if entry.runtime.set(runtime.clone()).is_err() {
        return entry.runtime.get().cloned();
    }
    Some(runtime)
}

async fn ensure_backfill_started(runtime: StateDbHandle, config: RolloutConfig) {
    let key = config.sqlite_home.clone();
    {
        let mut checked = lock_no_poison(backfill_checked_homes());
        if !checked.insert(key.clone()) {
            return;
        }
    }

    let backfill_state = match runtime.get_backfill_state().await {
        Ok(state) => state,
        Err(err) => {
            warn!(
                "failed to read backfill state at {}: {err}",
                config.praxis_home.display()
            );
            lock_no_poison(backfill_checked_homes()).remove(&key);
            return;
        }
    };
    let backfill_sessions = backfill_state.status != praxis_state::BackfillStatus::Complete;

    let runtime_for_backfill = runtime.clone();
    tokio::spawn(async move {
        if backfill_sessions {
            metadata::backfill_sessions(runtime_for_backfill.as_ref(), &config).await;
        }
        let complete = runtime_for_backfill
            .get_backfill_state()
            .await
            .map(|state| state.status == praxis_state::BackfillStatus::Complete)
            .unwrap_or(false);
        if !complete {
            lock_no_poison(backfill_checked_homes()).remove(&key);
        }
    });
}

fn cursor_to_anchor(cursor: Option<&Cursor>) -> Option<praxis_state::Anchor> {
    let cursor = cursor?;
    let value = serde_json::to_value(cursor).ok()?;
    let cursor_str = value.as_str()?;
    let (ts_str, id_str) = cursor_str.split_once('|')?;
    if id_str.contains('|') {
        return None;
    }
    let id = Uuid::parse_str(id_str).ok()?;
    let ts = if let Ok(naive) = NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H-%M-%S") {
        DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
    } else if let Ok(dt) = DateTime::parse_from_rfc3339(ts_str) {
        dt.with_timezone(&Utc)
    } else {
        return None;
    }
    .with_nanosecond(0)?;
    Some(praxis_state::Anchor { ts, id })
}

pub fn normalize_cwd_for_state_db(cwd: &Path) -> PathBuf {
    normalize_for_path_comparison(cwd).unwrap_or_else(|_| cwd.to_path_buf())
}

/// List thread ids from SQLite for parity checks without rollout scanning.
#[allow(clippy::too_many_arguments)]
pub async fn list_thread_ids_db(
    context: Option<&praxis_state::StateRuntime>,
    praxis_home: &Path,
    page_size: usize,
    cursor: Option<&Cursor>,
    sort_key: ThreadSortKey,
    allowed_sources: &[SessionSource],
    model_providers: Option<&[String]>,
    archived_only: bool,
    stage: &str,
) -> Option<Vec<ThreadId>> {
    let ctx = context?;
    if ctx.praxis_home() != praxis_home {
        warn!(
            "state db praxis_home mismatch: expected {}, got {}",
            ctx.praxis_home().display(),
            praxis_home.display()
        );
    }

    let anchor = cursor_to_anchor(cursor);
    let allowed_sources: Vec<String> = allowed_sources
        .iter()
        .map(|value| match serde_json::to_value(value) {
            Ok(Value::String(s)) => s,
            Ok(other) => other.to_string(),
            Err(_) => String::new(),
        })
        .collect();
    let model_providers = model_providers.map(<[String]>::to_vec);
    match ctx
        .list_thread_ids(
            page_size,
            anchor.as_ref(),
            match sort_key {
                ThreadSortKey::CreatedAt => praxis_state::SortKey::CreatedAt,
                ThreadSortKey::UpdatedAt => praxis_state::SortKey::UpdatedAt,
            },
            allowed_sources.as_slice(),
            model_providers.as_deref(),
            archived_only,
        )
        .await
    {
        Ok(ids) => Some(ids),
        Err(err) => {
            warn!("state db list_thread_ids failed during {stage}: {err}");
            None
        }
    }
}

/// List thread metadata from SQLite without rollout directory traversal.
#[allow(clippy::too_many_arguments)]
pub async fn list_threads_db(
    context: Option<&praxis_state::StateRuntime>,
    praxis_home: &Path,
    page_size: usize,
    cursor: Option<&Cursor>,
    sort_key: ThreadSortKey,
    allowed_sources: &[SessionSource],
    source_kinds: Option<&[praxis_state::ThreadSourceKind]>,
    model_providers: Option<&[String]>,
    archived: bool,
    cwd: Option<&Path>,
    search_term: Option<&str>,
) -> Option<praxis_state::ThreadsPage> {
    let ctx = context?;
    if ctx.praxis_home() != praxis_home {
        warn!(
            "state db praxis_home mismatch: expected {}, got {}",
            ctx.praxis_home().display(),
            praxis_home.display()
        );
    }

    let anchor = cursor_to_anchor(cursor);
    let allowed_sources: Vec<String> = allowed_sources
        .iter()
        .map(|value| match serde_json::to_value(value) {
            Ok(Value::String(s)) => s,
            Ok(other) => other.to_string(),
            Err(_) => String::new(),
        })
        .collect();
    let model_providers = model_providers.map(<[String]>::to_vec);
    let cwd = cwd
        .map(normalize_cwd_for_state_db)
        .map(|path| path.display().to_string());
    match ctx
        .list_threads(
            page_size,
            anchor.as_ref(),
            match sort_key {
                ThreadSortKey::CreatedAt => praxis_state::SortKey::CreatedAt,
                ThreadSortKey::UpdatedAt => praxis_state::SortKey::UpdatedAt,
            },
            allowed_sources.as_slice(),
            source_kinds,
            model_providers.as_deref(),
            archived,
            cwd.as_deref(),
            search_term,
        )
        .await
    {
        Ok(mut page) => {
            let mut valid_items = Vec::with_capacity(page.items.len());
            for item in page.items {
                if tokio::fs::try_exists(&item.rollout_path)
                    .await
                    .unwrap_or(false)
                {
                    valid_items.push(item);
                } else {
                    warn!(
                        "state db list_threads returned stale rollout path for thread {}: {}",
                        item.id,
                        item.rollout_path.display()
                    );
                    warn!("state db discrepancy during list_threads_db: stale_db_path_dropped");
                    let _ = ctx.delete_thread(item.id).await;
                }
            }
            page.items = valid_items;
            Some(page)
        }
        Err(err) => {
            warn!("state db list_threads failed: {err}");
            None
        }
    }
}

/// Look up the rollout path for a thread id using SQLite.
pub async fn find_rollout_path_by_id(
    context: Option<&praxis_state::StateRuntime>,
    thread_id: ThreadId,
    archived_only: Option<bool>,
    stage: &str,
) -> Option<PathBuf> {
    let ctx = context?;
    ctx.find_rollout_path_by_id(thread_id, archived_only)
        .await
        .unwrap_or_else(|err| {
            warn!("state db find_rollout_path_by_id failed during {stage}: {err}");
            None
        })
}

/// Get dynamic tools for a thread id using SQLite.
pub async fn get_dynamic_tools(
    context: Option<&praxis_state::StateRuntime>,
    thread_id: ThreadId,
    stage: &str,
) -> Option<Vec<DynamicToolSpec>> {
    let ctx = context?;
    match ctx.get_dynamic_tools(thread_id).await {
        Ok(tools) => tools,
        Err(err) => {
            warn!("state db get_dynamic_tools failed during {stage}: {err}");
            None
        }
    }
}

/// Persist dynamic tools for a thread id using SQLite, if none exist yet.
pub async fn persist_dynamic_tools(
    context: Option<&praxis_state::StateRuntime>,
    thread_id: ThreadId,
    tools: Option<&[DynamicToolSpec]>,
    stage: &str,
) {
    let Some(ctx) = context else {
        return;
    };
    if let Err(err) = ctx.persist_dynamic_tools(thread_id, tools).await {
        warn!("state db persist_dynamic_tools failed during {stage}: {err}");
    }
}

pub async fn mark_thread_memory_mode_polluted(
    context: Option<&praxis_state::StateRuntime>,
    thread_id: ThreadId,
    stage: &str,
) {
    let Some(ctx) = context else {
        return;
    };
    if let Err(err) = ctx.mark_thread_memory_mode_polluted(thread_id).await {
        warn!("state db mark_thread_memory_mode_polluted failed during {stage}: {err}");
    }
}

/// Reconcile rollout items into SQLite, falling back to scanning the rollout file.
pub async fn reconcile_rollout(
    context: Option<&praxis_state::StateRuntime>,
    rollout_path: &Path,
    default_provider: &str,
    builder: Option<&ThreadMetadataBuilder>,
    items: &[RolloutItem],
    archived_only: Option<bool>,
    new_thread_memory_mode: Option<&str>,
) {
    let Some(ctx) = context else {
        return;
    };
    if builder.is_some() || !items.is_empty() {
        apply_rollout_items(
            Some(ctx),
            rollout_path,
            default_provider,
            builder,
            items,
            "reconcile_rollout",
            new_thread_memory_mode,
            /*updated_at_override*/ None,
        )
        .await;
        return;
    }
    let outcome =
        match metadata::extract_metadata_from_rollout(rollout_path, default_provider).await {
            Ok(outcome) => outcome,
            Err(err) => {
                warn!(
                    "state db reconcile_rollout extraction failed {}: {err}",
                    rollout_path.display()
                );
                return;
            }
        };
    let mut metadata = outcome.metadata;
    let memory_mode = outcome.memory_mode.unwrap_or_else(|| "enabled".to_string());
    metadata.cwd = normalize_cwd_for_state_db(&metadata.cwd);
    if let Ok(Some(existing_metadata)) = ctx.get_thread(metadata.id).await {
        metadata.prefer_existing_git_info(&existing_metadata);
        metadata.selfwork_plan_path = existing_metadata.selfwork_plan_path.clone();
    }
    match archived_only {
        Some(true) if metadata.archived_at.is_none() => {
            metadata.archived_at = Some(metadata.updated_at);
        }
        Some(false) => {
            metadata.archived_at = None;
        }
        Some(true) | None => {}
    }
    if let Err(err) = ctx.upsert_thread(&metadata).await {
        warn!(
            "state db reconcile_rollout upsert failed {}: {err}",
            rollout_path.display()
        );
        return;
    }
    if let Err(err) = ctx
        .set_thread_memory_mode(metadata.id, memory_mode.as_str())
        .await
    {
        warn!(
            "state db reconcile_rollout memory_mode update failed {}: {err}",
            rollout_path.display()
        );
        return;
    }
    if let Ok(meta_line) = crate::list::read_session_meta_line(rollout_path).await {
        persist_dynamic_tools(
            Some(ctx),
            meta_line.meta.id,
            meta_line.meta.dynamic_tools.as_deref(),
            "reconcile_rollout",
        )
        .await;
    } else {
        warn!(
            "state db reconcile_rollout missing session meta {}",
            rollout_path.display()
        );
    }
}

/// Repair a thread's rollout path after filesystem fallback succeeds.
pub async fn read_repair_rollout_path(
    context: Option<&praxis_state::StateRuntime>,
    thread_id: Option<ThreadId>,
    archived_only: Option<bool>,
    rollout_path: &Path,
) {
    let Some(ctx) = context else {
        return;
    };

    // Fast path: update an existing metadata row in place, but avoid writes when
    // read-repair computes no effective change.
    let mut saw_existing_metadata = false;
    if let Some(thread_id) = thread_id
        && let Ok(Some(metadata)) = ctx.get_thread(thread_id).await
    {
        saw_existing_metadata = true;
        let mut repaired = metadata.clone();
        repaired.rollout_path = rollout_path.to_path_buf();
        repaired.cwd = normalize_cwd_for_state_db(&repaired.cwd);
        match archived_only {
            Some(true) if repaired.archived_at.is_none() => {
                repaired.archived_at = Some(repaired.updated_at);
            }
            Some(false) => {
                repaired.archived_at = None;
            }
            Some(true) | None => {}
        }
        if repaired == metadata {
            return;
        }
        warn!("state db discrepancy during read_repair_rollout_path: upsert_needed (fast path)");
        if let Err(err) = ctx.upsert_thread(&repaired).await {
            warn!(
                "state db read-repair upsert failed for {}: {err}",
                rollout_path.display()
            );
        } else {
            return;
        }
    }

    // Slow path: when the row is missing/unreadable (or direct upsert failed),
    // rebuild metadata from rollout contents and reconcile it into SQLite.
    if !saw_existing_metadata {
        warn!("state db discrepancy during read_repair_rollout_path: upsert_needed (slow path)");
    }
    let default_provider = crate::list::read_session_meta_line(rollout_path)
        .await
        .ok()
        .and_then(|meta| meta.meta.model_provider)
        .unwrap_or_default();
    reconcile_rollout(
        Some(ctx),
        rollout_path,
        default_provider.as_str(),
        /*builder*/ None,
        &[],
        archived_only,
        /*new_thread_memory_mode*/ None,
    )
    .await;
}

/// Apply rollout items incrementally to SQLite.
#[allow(clippy::too_many_arguments)]
pub async fn apply_rollout_items(
    context: Option<&praxis_state::StateRuntime>,
    rollout_path: &Path,
    _default_provider: &str,
    builder: Option<&ThreadMetadataBuilder>,
    items: &[RolloutItem],
    stage: &str,
    new_thread_memory_mode: Option<&str>,
    updated_at_override: Option<DateTime<Utc>>,
) {
    let Some(ctx) = context else {
        return;
    };
    let mut builder = match builder {
        Some(builder) => builder.clone(),
        None => match metadata::builder_from_items(items, rollout_path) {
            Some(builder) => builder,
            None => {
                warn!(
                    "state db apply_rollout_items missing builder during {stage}: {}",
                    rollout_path.display()
                );
                warn!("state db discrepancy during apply_rollout_items: {stage}, missing_builder");
                return;
            }
        },
    };
    builder.rollout_path = rollout_path.to_path_buf();
    builder.cwd = normalize_cwd_for_state_db(&builder.cwd);
    if let Err(err) = ctx
        .apply_rollout_items(&builder, items, new_thread_memory_mode, updated_at_override)
        .await
    {
        warn!(
            "state db apply_rollout_items failed during {stage} for {}: {err}",
            rollout_path.display()
        );
    }
}

pub async fn touch_thread_updated_at(
    context: Option<&praxis_state::StateRuntime>,
    thread_id: Option<ThreadId>,
    updated_at: DateTime<Utc>,
    stage: &str,
) -> bool {
    let Some(ctx) = context else {
        return false;
    };
    let Some(thread_id) = thread_id else {
        return false;
    };
    ctx.touch_thread_updated_at(thread_id, updated_at)
        .await
        .unwrap_or_else(|err| {
            warn!("state db touch_thread_updated_at failed during {stage} for {thread_id}: {err}");
            false
        })
}

#[cfg(test)]
#[path = "state_db_tests.rs"]
mod tests;
