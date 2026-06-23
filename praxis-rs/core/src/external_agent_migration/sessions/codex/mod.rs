use super::provider::ExternalSessionSyncStats;
use super::source::ExternalAgentSource;
use crate::config::Config;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tracing::warn;
use walkdir::WalkDir;

const SOURCE: ExternalAgentSource = ExternalAgentSource::Codex;

pub(super) async fn sync_sessions_to_store(
    config: &Config,
) -> io::Result<ExternalSessionSyncStats> {
    let source_home = match crate::config::default_external_codex_home() {
        Ok(path) => path,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(ExternalSessionSyncStats::default());
        }
        Err(err) => return Err(err),
    };
    if source_home == config.praxis_home {
        return Ok(ExternalSessionSyncStats::default());
    }

    let state_db = praxis_rollout::state_db::try_get_state_db(config)
        .await
        .ok();
    let mut stats = ExternalSessionSyncStats::default();
    for (subdir, archived) in [
        (praxis_rollout::SESSIONS_SUBDIR, false),
        (praxis_rollout::ARCHIVED_SESSIONS_SUBDIR, true),
    ] {
        sync_rollout_tree(
            &source_home.join(subdir),
            &config.praxis_home.join(subdir),
            archived,
            state_db.as_deref(),
            &mut stats,
        )
        .await?;
    }
    Ok(stats)
}

async fn sync_rollout_tree(
    source_root: &Path,
    dest_root: &Path,
    archived: bool,
    state_db: Option<&praxis_state::StateRuntime>,
    stats: &mut ExternalSessionSyncStats,
) -> io::Result<()> {
    if !source_root.exists() {
        return Ok(());
    }

    for entry in WalkDir::new(source_root) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warn!(
                    "failed to scan Codex session source {}: {err}",
                    source_root.display()
                );
                stats.skip_one();
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_rollout_file(entry.path()) {
            continue;
        }
        stats.discovered += 1;
        let relative_path = match entry.path().strip_prefix(source_root) {
            Ok(relative_path) => relative_path,
            Err(err) => {
                warn!(
                    "failed to resolve Codex rollout relative path {}: {err}",
                    entry.path().display()
                );
                stats.skip_one();
                continue;
            }
        };
        let dest_path = dest_root.join(relative_path);
        if copy_if_changed(entry.path(), &dest_path).await? {
            reconcile_rollout(&dest_path, archived, state_db).await;
            stats.import_one();
        } else {
            reconcile_rollout(&dest_path, archived, state_db).await;
            stats.skip_one();
        }
    }
    Ok(())
}

fn is_rollout_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
}

async fn copy_if_changed(source: &Path, dest: &Path) -> io::Result<bool> {
    if let (Ok(source_meta), Ok(dest_meta)) = (fs::metadata(source).await, fs::metadata(dest).await)
        && source_meta.len() == dest_meta.len()
        && source_meta
            .modified()
            .ok()
            .zip(dest_meta.modified().ok())
            .is_some_and(|(source_modified, dest_modified)| dest_modified >= source_modified)
    {
        return Ok(false);
    }

    let parent = dest.parent().ok_or_else(|| {
        io::Error::other(format!(
            "Codex bridge rollout destination has no parent: {}",
            dest.display()
        ))
    })?;
    fs::create_dir_all(parent).await?;
    copy_via_temp_file(source, dest).await?;
    Ok(true)
}

async fn copy_via_temp_file(source: &Path, dest: &Path) -> io::Result<()> {
    let temp_path = temp_path_for(dest);
    if let Some(parent) = temp_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    match fs::remove_file(&temp_path).await {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    fs::copy(source, &temp_path).await?;
    match fs::remove_file(dest).await {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    fs::rename(&temp_path, dest).await
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("rollout")
        .to_string();
    file_name.push_str(".tmp");
    path.with_file_name(file_name)
}

async fn reconcile_rollout(
    path: &Path,
    archived: bool,
    state_db: Option<&praxis_state::StateRuntime>,
) {
    let items = match praxis_rollout::RolloutRecorder::load_rollout_items(path).await {
        Ok((items, _, _)) => items,
        Err(err) => {
            warn!(
                "failed to load Codex bridge rollout {}: {err}",
                path.display()
            );
            Vec::new()
        }
    };
    praxis_rollout::state_db::reconcile_rollout(
        state_db,
        path,
        SOURCE.import_model_provider_id(),
        None,
        &items,
        Some(archived),
        Some("disabled"),
    )
    .await;
}
