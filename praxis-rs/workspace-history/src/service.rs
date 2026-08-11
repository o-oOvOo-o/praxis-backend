use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use fs2::FileExt;
use praxis_protocol::workspace_history::WorkspaceCheckpointFileSummary;
use praxis_protocol::workspace_history::WorkspaceCheckpointId;
use praxis_protocol::workspace_history::WorkspaceCheckpointRef;
use praxis_protocol::workspace_history::WorkspaceMutationKind;
use sqlx::Row;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqliteJournalMode;
use sqlx::sqlite::SqlitePoolOptions;
use walkdir::DirEntry;
use walkdir::WalkDir;

use crate::blob_store::BlobStore;
use crate::config::WorkspaceHistoryConfig;
use crate::manifest::WorkspaceCheckpointManifest;
use crate::manifest::WorkspaceFileVersion;

const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Default)]
struct CaptureCancellation {
    cancelled: Arc<AtomicBool>,
}

impl CaptureCancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn ensure_running(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            anyhow::bail!("workspace checkpoint capture cancelled");
        }
        Ok(())
    }
}

struct CancelCaptureOnDrop(CaptureCancellation);

impl Drop for CancelCaptureOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Debug, Clone)]
pub struct CaptureCheckpointRequest {
    pub workspace_root: PathBuf,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RestoreCheckpointOutcome {
    pub restored_checkpoint: WorkspaceCheckpointId,
    pub safety_checkpoint: WorkspaceCheckpointRef,
    pub restored_files: u32,
    pub removed_files: u32,
}

#[derive(Clone)]
pub struct WorkspaceHistoryService {
    root: Arc<PathBuf>,
    config: Arc<WorkspaceHistoryConfig>,
    pool: SqlitePool,
}

impl WorkspaceHistoryService {
    pub async fn open(
        praxis_home: impl AsRef<Path>,
        config: WorkspaceHistoryConfig,
    ) -> Result<Self> {
        let root = praxis_home.as_ref().join("workspace-history").join("v1");
        tokio::fs::create_dir_all(root.join("blobs")).await?;
        tokio::fs::create_dir_all(root.join("manifests")).await?;
        let options = SqliteConnectOptions::new()
            .filename(root.join("index.sqlite"))
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS checkpoints (\
             id TEXT PRIMARY KEY, workspace_root TEXT NOT NULL, thread_id TEXT, turn_id TEXT, \
             operation_id TEXT, created_at_ms INTEGER NOT NULL, manifest_path TEXT NOT NULL, \
             file_count INTEGER NOT NULL, changed_file_count INTEGER NOT NULL DEFAULT 0, \
             pinned INTEGER NOT NULL DEFAULT 0)",
        )
        .execute(&pool)
        .await?;
        let columns = sqlx::query("PRAGMA table_info(checkpoints)")
            .fetch_all(&pool)
            .await?;
        if !columns.iter().any(|row| {
            row.try_get::<String, _>("name").ok().as_deref() == Some("changed_file_count")
        }) {
            sqlx::query(
                "ALTER TABLE checkpoints ADD COLUMN changed_file_count INTEGER NOT NULL DEFAULT 0",
            )
            .execute(&pool)
            .await?;
        }
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS checkpoints_workspace_created \
             ON checkpoints(workspace_root, created_at_ms)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS checkpoints_thread_turn \
             ON checkpoints(thread_id, turn_id, created_at_ms)",
        )
        .execute(&pool)
        .await?;
        Ok(Self {
            root: Arc::new(root),
            config: Arc::new(config),
            pool,
        })
    }

    pub async fn capture(
        &self,
        request: CaptureCheckpointRequest,
    ) -> Result<WorkspaceCheckpointRef> {
        let cancellation = CaptureCancellation::default();
        let _cancel_on_drop = CancelCaptureOnDrop(cancellation.clone());
        let root = canonical_workspace_root(&request.workspace_root)?;
        let previous = self.latest_for_workspace(&root).await?;
        let id = WorkspaceCheckpointId::new();
        let created_at_unix_ms = Utc::now().timestamp_millis();
        let service_root = Arc::clone(&self.root);
        let config = Arc::clone(&self.config);
        let manifest_request = request.clone();
        let manifest = tokio::task::spawn_blocking(move || {
            capture_manifest_cancellable(
                service_root.as_ref(),
                config.as_ref(),
                root,
                id,
                created_at_unix_ms,
                manifest_request,
                cancellation,
            )
        })
        .await??;
        let manifest_path = self.manifest_path(id);
        let changed_file_count = match previous {
            Some(previous) => match self.summarize_changes(previous.id, id).await {
                Ok(changes) => u32::try_from(changes.len()).unwrap_or(u32::MAX),
                Err(error) => {
                    tracing::warn!(
                        previous_checkpoint_id = %previous.id,
                        checkpoint_id = %id,
                        "failed to compare workspace checkpoints; conservatively reporting all files changed: {error}"
                    );
                    u32::try_from(manifest.files.len()).unwrap_or(u32::MAX)
                }
            },
            None => u32::try_from(manifest.files.len()).unwrap_or(u32::MAX),
        };
        sqlx::query(
            "INSERT INTO checkpoints \
             (id, workspace_root, thread_id, turn_id, operation_id, created_at_ms, manifest_path, \
              file_count, changed_file_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(path_key(&manifest.workspace_root))
        .bind(manifest.thread_id.as_deref())
        .bind(manifest.turn_id.as_deref())
        .bind(manifest.operation_id.as_deref())
        .bind(created_at_unix_ms)
        .bind(manifest_path.to_string_lossy().as_ref())
        .bind(i64::try_from(manifest.files.len()).unwrap_or(i64::MAX))
        .bind(i64::from(changed_file_count))
        .execute(&self.pool)
        .await?;
        Ok(WorkspaceCheckpointRef {
            id,
            workspace_root: manifest.workspace_root,
            thread_id: manifest.thread_id,
            turn_id: manifest.turn_id,
            created_at_unix_ms,
            changed_file_count,
        })
    }

    pub async fn manifest(&self, id: WorkspaceCheckpointId) -> Result<WorkspaceCheckpointManifest> {
        let bytes = tokio::fs::read(self.manifest_path(id)).await?;
        let manifest: WorkspaceCheckpointManifest = serde_json::from_slice(&bytes)?;
        if manifest.schema_version != SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported workspace checkpoint schema version {}",
                manifest.schema_version
            );
        }
        if manifest.id != id {
            anyhow::bail!("workspace checkpoint manifest id does not match requested id");
        }
        Ok(manifest)
    }

    pub async fn summarize_changes(
        &self,
        before: WorkspaceCheckpointId,
        after: WorkspaceCheckpointId,
    ) -> Result<Vec<WorkspaceCheckpointFileSummary>> {
        let before = self.manifest(before).await?;
        let after = self.manifest(after).await?;
        ensure_same_workspace(&before, &after)?;
        Ok(summarize_manifest_changes(&before, &after))
    }

    pub async fn latest_for_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<Option<WorkspaceCheckpointRef>> {
        let row = sqlx::query(
            "SELECT id, workspace_root, created_at_ms, changed_file_count FROM checkpoints \
             WHERE thread_id = ? AND turn_id = ? ORDER BY created_at_ms DESC, rowid DESC LIMIT 1",
        )
        .bind(thread_id)
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| checkpoint_ref_from_row(row, thread_id, turn_id))
            .transpose()
    }

    pub async fn checkpoint_for_rewind(
        &self,
        thread_id: &str,
        num_turns: u32,
    ) -> Result<Option<WorkspaceCheckpointRef>> {
        if num_turns == 0 {
            anyhow::bail!("num_turns must be >= 1");
        }
        let row = sqlx::query(
            "SELECT id, workspace_root, thread_id, turn_id, created_at_ms, changed_file_count \
             FROM checkpoints WHERE thread_id = ? AND operation_id = 'turn-boundary' \
             ORDER BY created_at_ms DESC, rowid DESC LIMIT 1 OFFSET ?",
        )
        .bind(thread_id)
        .bind(i64::from(num_turns - 1))
        .fetch_optional(&self.pool)
        .await?;
        row.map(checkpoint_ref_from_optional_row).transpose()
    }

    pub async fn latest_for_workspace(
        &self,
        workspace_root: &Path,
    ) -> Result<Option<WorkspaceCheckpointRef>> {
        let root = canonical_workspace_root(workspace_root)?;
        let row = sqlx::query(
            "SELECT id, workspace_root, thread_id, turn_id, created_at_ms, changed_file_count FROM checkpoints \
             WHERE workspace_root = ? ORDER BY created_at_ms DESC, rowid DESC LIMIT 1",
        )
        .bind(path_key(&root))
        .fetch_optional(&self.pool)
        .await?;
        row.map(checkpoint_ref_from_optional_row).transpose()
    }

    pub async fn restore(
        &self,
        id: WorkspaceCheckpointId,
        thread_id: Option<String>,
        turn_id: Option<String>,
    ) -> Result<RestoreCheckpointOutcome> {
        let target = self.manifest(id).await?;
        let safety_checkpoint = self
            .capture(CaptureCheckpointRequest {
                workspace_root: target.workspace_root.clone(),
                thread_id,
                turn_id,
                operation_id: Some(format!("restore-safety:{id}")),
            })
            .await?;
        let service_root = Arc::clone(&self.root);
        let config = Arc::clone(&self.config);
        let restore_result = tokio::task::spawn_blocking(move || {
            restore_manifest(service_root.as_ref(), config.as_ref(), &target)
        })
        .await?;
        let (restored_files, removed_files) = match restore_result {
            Ok(outcome) => outcome,
            Err(restore_error) => {
                let safety_manifest = self.manifest(safety_checkpoint.id).await?;
                let service_root = Arc::clone(&self.root);
                let config = Arc::clone(&self.config);
                let recovery = tokio::task::spawn_blocking(move || {
                    restore_manifest(service_root.as_ref(), config.as_ref(), &safety_manifest)
                })
                .await?;
                match recovery {
                    Ok(_) => anyhow::bail!(
                        "workspace restore failed and the pre-restore workspace was recovered: {restore_error}"
                    ),
                    Err(recovery_error) => anyhow::bail!(
                        "workspace restore failed: {restore_error}; safety recovery also failed: {recovery_error}"
                    ),
                }
            }
        };
        Ok(RestoreCheckpointOutcome {
            restored_checkpoint: id,
            safety_checkpoint,
            restored_files,
            removed_files,
        })
    }

    pub async fn prune(&self) -> Result<()> {
        let maintenance_lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(self.root.join("maintenance.lock"))?;
        match maintenance_lock.try_lock_exclusive() {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let cutoff = Utc::now().timestamp_millis()
            - i64::from(self.config.retention_days) * 24 * 60 * 60 * 1_000;
        let stale = sqlx::query(
            "SELECT id, manifest_path FROM checkpoints \
             WHERE pinned = 0 AND created_at_ms < ? ORDER BY created_at_ms ASC",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;
        for row in stale {
            let id: String = row.try_get("id")?;
            let manifest_path: String = row.try_get("manifest_path")?;
            let _ = tokio::fs::remove_file(manifest_path).await;
            sqlx::query("DELETE FROM checkpoints WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        self.prune_unreferenced_blobs().await?;
        while self.store_size_bytes().await? > self.config.max_store_bytes {
            let oldest = sqlx::query(
                "SELECT id, manifest_path FROM checkpoints WHERE pinned = 0 \
                 ORDER BY created_at_ms ASC LIMIT 1",
            )
            .fetch_optional(&self.pool)
            .await?;
            let Some(oldest) = oldest else {
                break;
            };
            let id: String = oldest.try_get("id")?;
            let manifest_path: String = oldest.try_get("manifest_path")?;
            let _ = tokio::fs::remove_file(manifest_path).await;
            sqlx::query("DELETE FROM checkpoints WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
            self.prune_unreferenced_blobs().await?;
        }
        let _ = maintenance_lock.unlock();
        Ok(())
    }

    async fn prune_unreferenced_blobs(&self) -> Result<()> {
        let mut referenced = HashSet::new();
        let rows = sqlx::query("SELECT manifest_path FROM checkpoints")
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            let path: String = row.try_get("manifest_path")?;
            let bytes = tokio::fs::read(&path)
                .await
                .with_context(|| format!("read referenced workspace manifest {path}"))?;
            let manifest: WorkspaceCheckpointManifest = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse referenced workspace manifest {path}"))?;
            referenced.extend(manifest.files.into_iter().map(|file| file.blob_hash));
        }
        let blob_root = self.root.join("blobs");
        tokio::task::spawn_blocking(move || -> Result<()> {
            for entry in WalkDir::new(&blob_root).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy();
                let hash = name.strip_suffix(".zst").unwrap_or(&name).to_owned();
                if referenced.contains(&hash) {
                    continue;
                }
                fs::remove_file(entry.path())?;
            }
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn store_size_bytes(&self) -> Result<u64> {
        let root = Arc::clone(&self.root);
        tokio::task::spawn_blocking(move || {
            WalkDir::new(root.as_ref())
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .try_fold(0u64, |total, entry| {
                    Ok::<_, anyhow::Error>(total.saturating_add(entry.metadata()?.len()))
                })
        })
        .await?
    }

    fn manifest_path(&self, id: WorkspaceCheckpointId) -> PathBuf {
        self.root.join("manifests").join(format!("{id}.json"))
    }
}

#[cfg(test)]
fn capture_manifest(
    service_root: &Path,
    config: &WorkspaceHistoryConfig,
    workspace_root: PathBuf,
    id: WorkspaceCheckpointId,
    created_at_unix_ms: i64,
    request: CaptureCheckpointRequest,
) -> Result<WorkspaceCheckpointManifest> {
    capture_manifest_cancellable(
        service_root,
        config,
        workspace_root,
        id,
        created_at_unix_ms,
        request,
        CaptureCancellation::default(),
    )
}

fn capture_manifest_cancellable(
    service_root: &Path,
    config: &WorkspaceHistoryConfig,
    workspace_root: PathBuf,
    id: WorkspaceCheckpointId,
    created_at_unix_ms: i64,
    request: CaptureCheckpointRequest,
    cancellation: CaptureCancellation,
) -> Result<WorkspaceCheckpointManifest> {
    cancellation.ensure_running()?;
    let blob_store = BlobStore::new(service_root.join("blobs"));
    let mut files = Vec::new();
    let mut skipped_files = Vec::new();
    let walker = WalkDir::new(&workspace_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_visit(entry, config));
    for entry in walker {
        cancellation.ensure_running()?;
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        let relative = entry.path().strip_prefix(&workspace_root)?.to_path_buf();
        if metadata.len() > config.max_file_bytes {
            skipped_files.push(relative);
            continue;
        }
        cancellation.ensure_running()?;
        let modified_at_unix_ns = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let executable = is_executable(&metadata);
        // Size and mtime can both remain unchanged after a real edit. Reusing the prior blob
        // based on metadata would silently create a stale checkpoint.
        let blob_hash = blob_store.put(&fs::read(entry.path())?)?;
        files.push(WorkspaceFileVersion {
            path: relative,
            blob_hash,
            byte_size: metadata.len(),
            modified_at_unix_ns,
            executable,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    skipped_files.sort();
    cancellation.ensure_running()?;
    let manifest = WorkspaceCheckpointManifest {
        schema_version: SCHEMA_VERSION,
        id,
        workspace_root,
        thread_id: request.thread_id,
        turn_id: request.turn_id,
        operation_id: request.operation_id,
        created_at_unix_ms,
        files,
        skipped_files,
    };
    let manifest_path = service_root.join("manifests").join(format!("{id}.json"));
    let parent = manifest_path
        .parent()
        .context("manifest path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(temp.as_file(), &manifest)?;
    temp.as_file().sync_all()?;
    temp.persist(&manifest_path).map_err(|error| error.error)?;
    Ok(manifest)
}

fn restore_manifest(
    service_root: &Path,
    config: &WorkspaceHistoryConfig,
    manifest: &WorkspaceCheckpointManifest,
) -> Result<(u32, u32)> {
    let lock_path = service_root.join("workspace.lock");
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    let result = restore_manifest_locked(service_root, config, manifest);
    let _ = lock.unlock();
    result
}

fn restore_manifest_locked(
    service_root: &Path,
    config: &WorkspaceHistoryConfig,
    manifest: &WorkspaceCheckpointManifest,
) -> Result<(u32, u32)> {
    let root = canonical_workspace_root(&manifest.workspace_root)?;
    let mut target = BTreeMap::<&Path, &WorkspaceFileVersion>::new();
    for file in &manifest.files {
        validate_relative_checkpoint_path(&file.path)?;
        if file.byte_size > config.max_file_bytes {
            anyhow::bail!("workspace checkpoint file exceeds configured restore limit");
        }
        if target.insert(file.path.as_path(), file).is_some() {
            anyhow::bail!("workspace checkpoint contains duplicate file path");
        }
    }
    let mut skipped = HashSet::<&Path>::new();
    for path in &manifest.skipped_files {
        validate_relative_checkpoint_path(path)?;
        skipped.insert(path.as_path());
    }

    // Read and stage every blob before replacing or deleting workspace data. A corrupt or
    // incomplete checkpoint must fail without leaving the workspace half-restored.
    let blob_store = BlobStore::new(service_root.join("blobs"));
    let mut staged = Vec::with_capacity(manifest.files.len());
    for file in &manifest.files {
        let destination = safe_destination(&root, &file.path)?;
        let parent = destination.parent().context("destination has no parent")?;
        fs::create_dir_all(parent)?;
        let bytes = blob_store.get_limited(&file.blob_hash, file.byte_size)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        use std::io::Write as _;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        staged.push((temp, destination, file.executable));
    }

    let mut restored_files = 0u32;
    for (temp, destination, executable) in staged {
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                anyhow::bail!(
                    "workspace checkpoint destination is not a regular file: {}",
                    destination.display()
                );
            }
            Ok(_) => fs::remove_file(&destination)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        temp.persist(&destination).map_err(|error| error.error)?;
        restore_executable_bit(&destination, executable)?;
        restored_files = restored_files.saturating_add(1);
    }

    let mut removed_files = 0u32;
    let walker = WalkDir::new(&root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_entry(|entry| should_visit(entry, config));
    for entry in walker {
        let entry = entry?;
        let Ok(relative) = entry.path().strip_prefix(&root) else {
            continue;
        };
        if entry.file_type().is_file()
            && !target.contains_key(relative)
            && !skipped.contains(relative)
        {
            fs::remove_file(entry.path())?;
            removed_files = removed_files.saturating_add(1);
        } else if entry.file_type().is_dir()
            && entry.path() != root
            && fs::read_dir(entry.path())?.next().is_none()
        {
            let _ = fs::remove_dir(entry.path());
        }
    }

    Ok((restored_files, removed_files))
}

fn summarize_manifest_changes(
    before: &WorkspaceCheckpointManifest,
    after: &WorkspaceCheckpointManifest,
) -> Vec<WorkspaceCheckpointFileSummary> {
    let before_files: BTreeMap<&Path, &WorkspaceFileVersion> = before
        .files
        .iter()
        .map(|file| (file.path.as_path(), file))
        .collect();
    let after_files: BTreeMap<&Path, &WorkspaceFileVersion> = after
        .files
        .iter()
        .map(|file| (file.path.as_path(), file))
        .collect();
    let paths: HashSet<&Path> = before_files
        .keys()
        .copied()
        .chain(after_files.keys().copied())
        .collect();
    let mut summaries = paths
        .into_iter()
        .filter_map(
            |path| match (before_files.get(path), after_files.get(path)) {
                (None, Some(after)) => Some(WorkspaceCheckpointFileSummary {
                    path: path.to_path_buf(),
                    previous_path: None,
                    kind: WorkspaceMutationKind::Add,
                    byte_size: after.byte_size,
                }),
                (Some(before), None) => Some(WorkspaceCheckpointFileSummary {
                    path: path.to_path_buf(),
                    previous_path: None,
                    kind: WorkspaceMutationKind::Delete,
                    byte_size: before.byte_size,
                }),
                (Some(before), Some(after)) if before.blob_hash != after.blob_hash => {
                    Some(WorkspaceCheckpointFileSummary {
                        path: path.to_path_buf(),
                        previous_path: None,
                        kind: WorkspaceMutationKind::Update,
                        byte_size: after.byte_size,
                    })
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    pair_renames(&mut summaries, &before_files, &after_files);
    summaries.sort_by(|left, right| left.path.cmp(&right.path));
    summaries
}

fn pair_renames(
    summaries: &mut Vec<WorkspaceCheckpointFileSummary>,
    before: &BTreeMap<&Path, &WorkspaceFileVersion>,
    after: &BTreeMap<&Path, &WorkspaceFileVersion>,
) {
    let mut deleted_by_hash = BTreeMap::<&str, Vec<PathBuf>>::new();
    for summary in summaries
        .iter()
        .filter(|item| item.kind == WorkspaceMutationKind::Delete)
    {
        if let Some(file) = before.get(summary.path.as_path()) {
            deleted_by_hash
                .entry(&file.blob_hash)
                .or_default()
                .push(summary.path.clone());
        }
    }
    let mut consumed_deletes = HashSet::new();
    for summary in summaries
        .iter_mut()
        .filter(|item| item.kind == WorkspaceMutationKind::Add)
    {
        let Some(file) = after.get(summary.path.as_path()) else {
            continue;
        };
        let Some(candidates) = deleted_by_hash.get_mut(file.blob_hash.as_str()) else {
            continue;
        };
        let Some(previous_path) = candidates.pop() else {
            continue;
        };
        consumed_deletes.insert(previous_path.clone());
        summary.previous_path = Some(previous_path);
        summary.kind = WorkspaceMutationKind::Rename;
    }
    summaries.retain(|item| {
        item.kind != WorkspaceMutationKind::Delete || !consumed_deletes.contains(&item.path)
    });
}

fn checkpoint_ref_from_row(
    row: sqlx::sqlite::SqliteRow,
    thread_id: &str,
    turn_id: &str,
) -> Result<WorkspaceCheckpointRef> {
    let id: String = row.try_get("id")?;
    Ok(WorkspaceCheckpointRef {
        id: WorkspaceCheckpointId(uuid::Uuid::parse_str(&id)?),
        workspace_root: PathBuf::from(row.try_get::<String, _>("workspace_root")?),
        thread_id: Some(thread_id.to_owned()),
        turn_id: Some(turn_id.to_owned()),
        created_at_unix_ms: row.try_get("created_at_ms")?,
        changed_file_count: u32::try_from(row.try_get::<i64, _>("changed_file_count")?)
            .unwrap_or(u32::MAX),
    })
}

fn checkpoint_ref_from_optional_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<WorkspaceCheckpointRef> {
    let id: String = row.try_get("id")?;
    Ok(WorkspaceCheckpointRef {
        id: WorkspaceCheckpointId(uuid::Uuid::parse_str(&id)?),
        workspace_root: PathBuf::from(row.try_get::<String, _>("workspace_root")?),
        thread_id: row.try_get("thread_id")?,
        turn_id: row.try_get("turn_id")?,
        created_at_unix_ms: row.try_get("created_at_ms")?,
        changed_file_count: u32::try_from(row.try_get::<i64, _>("changed_file_count")?)
            .unwrap_or(u32::MAX),
    })
}

fn canonical_workspace_root(path: &Path) -> Result<PathBuf> {
    let root = path
        .canonicalize()
        .with_context(|| format!("resolve workspace root {}", path.display()))?;
    if !root.is_dir() {
        anyhow::bail!("workspace root is not a directory: {}", root.display());
    }
    Ok(root)
}

fn should_visit(entry: &DirEntry, config: &WorkspaceHistoryConfig) -> bool {
    entry.depth() == 0
        || !entry.file_type().is_dir()
        || !config
            .ignored_directory_names
            .contains(&entry.file_name().to_string_lossy().to_string())
}

fn path_key(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

fn ensure_same_workspace(
    left: &WorkspaceCheckpointManifest,
    right: &WorkspaceCheckpointManifest,
) -> Result<()> {
    if path_key(&left.workspace_root) != path_key(&right.workspace_root) {
        anyhow::bail!("workspace checkpoint roots do not match");
    }
    Ok(())
}

fn validate_relative_checkpoint_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        anyhow::bail!(
            "checkpoint path must be a normalized relative path: {}",
            path.display()
        );
    }
    Ok(())
}

fn safe_destination(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_checkpoint_path(relative)?;
    let mut destination = root.to_path_buf();
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(name) = component else {
            unreachable!("relative checkpoint path was validated");
        };
        destination.push(name);
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!(
                    "checkpoint path traverses a symbolic link: {}",
                    destination.display()
                );
            }
            Ok(metadata) if !metadata.is_dir() => {
                anyhow::bail!(
                    "checkpoint path parent is not a directory: {}",
                    destination.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    destination.push(
        relative
            .file_name()
            .context("checkpoint path has no file name")?,
    );
    Ok(destination)
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn restore_executable_bit(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    let mut mode = permissions.mode();
    if executable {
        mode |= 0o111;
    } else {
        mode &= !0o111;
    }
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn restore_executable_bit(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture_for_test(
        service_root: &Path,
        workspace_root: &Path,
        config: &WorkspaceHistoryConfig,
    ) -> WorkspaceCheckpointManifest {
        capture_manifest(
            service_root,
            config,
            workspace_root.to_path_buf(),
            WorkspaceCheckpointId::new(),
            1,
            CaptureCheckpointRequest {
                workspace_root: workspace_root.to_path_buf(),
                thread_id: None,
                turn_id: None,
                operation_id: None,
            },
        )
        .expect("capture checkpoint")
    }

    #[test]
    fn cancelled_capture_stops_before_writing_a_manifest() {
        let service = tempfile::tempdir().expect("service tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        fs::create_dir_all(service.path().join("blobs")).expect("blob dir");
        fs::create_dir_all(service.path().join("manifests")).expect("manifest dir");
        fs::write(workspace.path().join("tracked.txt"), b"content").expect("tracked file");
        let cancellation = CaptureCancellation::default();
        cancellation.cancel();

        let result = capture_manifest_cancellable(
            service.path(),
            &WorkspaceHistoryConfig::default(),
            workspace.path().to_path_buf(),
            WorkspaceCheckpointId::new(),
            1,
            CaptureCheckpointRequest {
                workspace_root: workspace.path().to_path_buf(),
                thread_id: None,
                turn_id: None,
                operation_id: None,
            },
            cancellation,
        );

        assert!(
            result
                .expect_err("cancelled capture must fail")
                .to_string()
                .contains("cancelled")
        );
        assert_eq!(
            fs::read_dir(service.path().join("manifests"))
                .expect("manifest dir")
                .count(),
            0
        );
    }

    #[test]
    fn dropping_capture_guard_cancels_the_blocking_worker() {
        let cancellation = CaptureCancellation::default();
        let worker_cancellation = cancellation.clone();

        drop(CancelCaptureOnDrop(cancellation));

        assert!(worker_cancellation.ensure_running().is_err());
    }

    #[test]
    fn restore_preserves_files_that_capture_skipped() {
        let service = tempfile::tempdir().expect("service tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let config = WorkspaceHistoryConfig {
            max_file_bytes: 4,
            ..WorkspaceHistoryConfig::default()
        };
        fs::create_dir_all(service.path().join("blobs")).expect("blob dir");
        fs::create_dir_all(service.path().join("manifests")).expect("manifest dir");
        fs::write(workspace.path().join("small.txt"), b"old").expect("small file");
        fs::write(workspace.path().join("large.bin"), b"large-file").expect("large file");
        let manifest = capture_for_test(service.path(), workspace.path(), &config);

        fs::write(workspace.path().join("small.txt"), b"new").expect("modify small file");
        fs::write(workspace.path().join("large.bin"), b"keep-this-large-file")
            .expect("modify skipped file");
        fs::write(workspace.path().join("extra.txt"), b"extra").expect("extra file");

        restore_manifest(service.path(), &config, &manifest).expect("restore checkpoint");

        assert_eq!(
            fs::read(workspace.path().join("small.txt")).unwrap(),
            b"old"
        );
        assert_eq!(
            fs::read(workspace.path().join("large.bin")).unwrap(),
            b"keep-this-large-file"
        );
        assert!(!workspace.path().join("extra.txt").exists());
    }

    #[test]
    fn corrupt_blob_fails_before_workspace_files_are_changed() {
        let service = tempfile::tempdir().expect("service tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let config = WorkspaceHistoryConfig::default();
        fs::create_dir_all(service.path().join("blobs")).expect("blob dir");
        fs::create_dir_all(service.path().join("manifests")).expect("manifest dir");
        fs::write(workspace.path().join("tracked.txt"), b"checkpoint").expect("tracked file");
        let manifest = capture_for_test(service.path(), workspace.path(), &config);
        let hash = &manifest.files[0].blob_hash;
        fs::remove_file(
            service
                .path()
                .join("blobs")
                .join(&hash[..2])
                .join(format!("{hash}.zst")),
        )
        .expect("remove blob");
        fs::write(workspace.path().join("tracked.txt"), b"current").expect("modify tracked");
        fs::write(workspace.path().join("extra.txt"), b"must survive").expect("extra file");

        assert!(restore_manifest(service.path(), &config, &manifest).is_err());
        assert_eq!(
            fs::read(workspace.path().join("tracked.txt")).unwrap(),
            b"current"
        );
        assert_eq!(
            fs::read(workspace.path().join("extra.txt")).unwrap(),
            b"must survive"
        );
    }

    #[test]
    fn checkpoint_paths_must_be_normalized_and_relative() {
        assert!(validate_relative_checkpoint_path(Path::new("src/lib.rs")).is_ok());
        assert!(validate_relative_checkpoint_path(Path::new("../outside.txt")).is_err());
        assert!(validate_relative_checkpoint_path(Path::new("src/../outside.txt")).is_err());
        assert!(validate_relative_checkpoint_path(Path::new("/absolute.txt")).is_err());
    }

    #[tokio::test]
    async fn capture_detects_content_changes_with_identical_size_and_mtime() {
        let service_home = tempfile::tempdir().expect("service tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let service =
            WorkspaceHistoryService::open(service_home.path(), WorkspaceHistoryConfig::default())
                .await
                .expect("open workspace history");
        let path = workspace.path().join("tracked.txt");
        fs::write(&path, b"first").expect("write first version");
        let original_modified = fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .expect("read original mtime");
        let first = service
            .capture(CaptureCheckpointRequest {
                workspace_root: workspace.path().to_path_buf(),
                thread_id: None,
                turn_id: None,
                operation_id: None,
            })
            .await
            .expect("capture first version");

        fs::write(&path, b"other").expect("write same-size second version");
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| {
                file.set_times(std::fs::FileTimes::new().set_modified(original_modified))
            })
            .expect("restore original mtime");
        let second = service
            .capture(CaptureCheckpointRequest {
                workspace_root: workspace.path().to_path_buf(),
                thread_id: None,
                turn_id: None,
                operation_id: None,
            })
            .await
            .expect("capture second version");

        let first_manifest = service.manifest(first.id).await.expect("first manifest");
        let second_manifest = service.manifest(second.id).await.expect("second manifest");
        assert_ne!(
            first_manifest.files[0].blob_hash,
            second_manifest.files[0].blob_hash
        );
    }

    #[tokio::test]
    async fn corrupt_previous_manifest_does_not_fail_new_capture() {
        let service_home = tempfile::tempdir().expect("service tempdir");
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let service =
            WorkspaceHistoryService::open(service_home.path(), WorkspaceHistoryConfig::default())
                .await
                .expect("open workspace history");
        let path = workspace.path().join("tracked.txt");
        fs::write(&path, b"first").expect("write first version");
        let first = service
            .capture(CaptureCheckpointRequest {
                workspace_root: workspace.path().to_path_buf(),
                thread_id: None,
                turn_id: None,
                operation_id: None,
            })
            .await
            .expect("capture first version");
        fs::write(service.manifest_path(first.id), b"not json").expect("corrupt previous manifest");
        fs::write(&path, b"second").expect("write second version");

        let second = service
            .capture(CaptureCheckpointRequest {
                workspace_root: workspace.path().to_path_buf(),
                thread_id: None,
                turn_id: None,
                operation_id: None,
            })
            .await
            .expect("new capture should not inherit prior corruption");

        assert_eq!(second.changed_file_count, 1);
        assert_eq!(
            service
                .manifest(second.id)
                .await
                .expect("new manifest")
                .files
                .len(),
            1
        );
    }
}
