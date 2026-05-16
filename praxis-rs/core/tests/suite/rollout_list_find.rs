#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use chrono::Utc;
use praxis_core::find_archived_thread_path_by_id_str;
use praxis_core::find_thread_path_by_id_str;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_state::StateRuntime;
use praxis_state::ThreadMetadataBuilder;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use uuid::Uuid;

/// Create <subdir>/YYYY/MM/DD and write a minimal rollout file containing the
/// provided conversation id in the SessionMeta line. Returns the absolute path.
fn write_minimal_rollout_with_id_in_subdir(praxis_home: &Path, subdir: &str, id: Uuid) -> PathBuf {
    let sessions = praxis_home.join(subdir).join("2024/01/01");
    std::fs::create_dir_all(&sessions).unwrap();

    let file = sessions.join(format!("rollout-2024-01-01T00-00-00-{id}.jsonl"));
    let mut f = std::fs::File::create(&file).unwrap();
    // Minimal first line: session_meta with the id so content search can find it
    writeln!(
        f,
        "{}",
        serde_json::json!({
            "timestamp": "2024-01-01T00:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": id,
                "timestamp": "2024-01-01T00:00:00Z",
                "cwd": ".",
                "originator": "test",
                "cli_version": "test",
                "model_provider": "test-provider"
            }
        })
    )
    .unwrap();

    file
}

/// Create sessions/YYYY/MM/DD and write a minimal rollout file containing the
/// provided conversation id in the SessionMeta line. Returns the absolute path.
fn write_minimal_rollout_with_id(praxis_home: &Path, id: Uuid) -> PathBuf {
    write_minimal_rollout_with_id_in_subdir(praxis_home, "sessions", id)
}

async fn upsert_thread_metadata(praxis_home: &Path, thread_id: ThreadId, rollout_path: PathBuf) {
    let runtime = StateRuntime::init(praxis_home.to_path_buf(), "test-provider".to_string())
        .await
        .unwrap();
    runtime
        .mark_backfill_complete(/*last_watermark*/ None)
        .await
        .unwrap();
    let mut builder = ThreadMetadataBuilder::new(
        thread_id,
        rollout_path,
        Utc::now(),
        SessionSource::default(),
    );
    builder.cwd = praxis_home.to_path_buf();
    let metadata = builder.build("test-provider");
    runtime.upsert_thread(&metadata).await.unwrap();
}

#[tokio::test]
async fn find_locates_rollout_file_by_id() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(home.path(), id);

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found.unwrap(), expected);
}

#[tokio::test]
async fn find_handles_gitignore_covering_praxis_home_directory() {
    let repo = TempDir::new().unwrap();
    let praxis_home = repo.path().join(".codex");
    std::fs::create_dir_all(&praxis_home).unwrap();
    std::fs::write(repo.path().join(".gitignore"), ".codex/**\n").unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(&praxis_home, id);

    let found = find_thread_path_by_id_str(&praxis_home, &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}

#[tokio::test]
async fn find_prefers_sqlite_path_by_id() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let thread_id = ThreadId::from_string(&id.to_string()).unwrap();
    let db_path = home.path().join(format!(
        "sessions/2030/12/30/rollout-2030-12-30T00-00-00-{id}.jsonl"
    ));
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    std::fs::write(&db_path, "").unwrap();
    write_minimal_rollout_with_id(home.path(), id);
    upsert_thread_metadata(home.path(), thread_id, db_path.clone()).await;

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(db_path));
}

#[tokio::test]
async fn find_falls_back_to_filesystem_when_sqlite_has_no_match() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(home.path(), id);
    let unrelated_id = Uuid::new_v4();
    let unrelated_thread_id = ThreadId::from_string(&unrelated_id.to_string()).unwrap();
    let unrelated_path = home
        .path()
        .join("sessions/2030/12/30/rollout-2030-12-30T00-00-00-unrelated.jsonl");
    upsert_thread_metadata(home.path(), unrelated_thread_id, unrelated_path).await;

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}

#[tokio::test]
async fn find_ignores_granular_gitignore_rules() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id(home.path(), id);
    std::fs::write(home.path().join("sessions/.gitignore"), "*.jsonl\n").unwrap();

    let found = find_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}

#[tokio::test]
async fn find_archived_locates_rollout_file_by_id() {
    let home = TempDir::new().unwrap();
    let id = Uuid::new_v4();
    let expected = write_minimal_rollout_with_id_in_subdir(home.path(), "archived_sessions", id);

    let found = find_archived_thread_path_by_id_str(home.path(), &id.to_string())
        .await
        .unwrap();

    assert_eq!(found, Some(expected));
}
