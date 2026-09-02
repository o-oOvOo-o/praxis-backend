use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::INTERACTIVE_SESSION_SOURCES;
use crate::RolloutConfig;
use crate::RolloutConfigView;
use crate::list::ThreadSortKey;
use crate::recorder::NativeRolloutWriter;

const MIGRATION_PAGE_SIZE: usize = 64;

fn active_homes() -> &'static Mutex<HashSet<PathBuf>> {
    static HOMES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    HOMES.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(super) fn ensure_started(config: &impl RolloutConfigView) {
    let config = RolloutConfig::from_view(config);
    let marker = completion_marker(config.praxis_home.as_path());
    if marker.exists() {
        return;
    }
    let key = config.praxis_home.clone();
    if !active_homes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key.clone())
    {
        return;
    }
    tokio::spawn(async move {
        if let Err(error) = migrate_all(&config).await {
            tracing::warn!(
                "native thread migration failed for {}: {error}",
                config.praxis_home.display()
            );
            active_homes()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&key);
        }
    });
}

pub(super) fn is_complete(praxis_home: &Path) -> bool {
    completion_marker(praxis_home).exists()
}

async fn migrate_all(config: &RolloutConfig) -> io::Result<()> {
    for archived in [false, true] {
        migrate_directory(config, archived).await?;
    }
    write_completion_marker(config.praxis_home.as_path()).await
}

async fn migrate_directory(config: &RolloutConfig, archived: bool) -> io::Result<()> {
    let mut cursor = None;
    let native_store =
        praxis_thread_store::ThreadStore::from_praxis_home(config.praxis_home.clone());
    loop {
        let page = super::directory::list_raw_threads(
            config,
            None,
            MIGRATION_PAGE_SIZE,
            cursor.as_ref(),
            ThreadSortKey::CreatedAt,
            &INTERACTIVE_SESSION_SOURCES,
            None,
            None,
            config.model_provider_id.as_str(),
            archived,
            None,
            None,
        )
        .await?;
        for item in page.items {
            let thread_id = item
                .thread_id
                .or_else(|| super::thread_id_from_rollout_path(item.path.as_path()));
            let writer =
                NativeRolloutWriter::resume(config.praxis_home.clone(), item.path.as_path())
                    .await?;
            writer.sync().await?;
            if archived && let Some(thread_id) = thread_id {
                let thread_id =
                    praxis_thread_store_contracts::ThreadId::parse(thread_id.to_string().as_str())
                        .map_err(|error| io::Error::other(error.to_string()))?;
                native_store
                    .set_archived(thread_id, true)
                    .await
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
        }
        let Some(next) = page.next_cursor else {
            return Ok(());
        };
        cursor = Some(next);
    }
}

fn completion_marker(praxis_home: &Path) -> PathBuf {
    praxis_home
        .join(praxis_thread_store::THREAD_STORE_SUBDIR)
        .join("rollout-import-v2.complete")
}

async fn write_completion_marker(praxis_home: &Path) -> io::Result<()> {
    let marker = completion_marker(praxis_home);
    let parent = marker
        .parent()
        .ok_or_else(|| io::Error::other("migration marker has no parent"))?;
    tokio::fs::create_dir_all(parent).await?;
    let temporary = parent.join(format!(".rollout-import-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = tokio::fs::OpenOptions::new();
    let mut file = options
        .create_new(true)
        .write(true)
        .open(&temporary)
        .await?;
    use tokio::io::AsyncWriteExt;
    file.write_all(b"2\n").await?;
    file.sync_all().await?;
    drop(file);
    match tokio::fs::rename(&temporary, &marker).await {
        Ok(()) => Ok(()),
        Err(_error) if marker.exists() => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Ok(())
        }
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Err(error)
        }
    }
}
