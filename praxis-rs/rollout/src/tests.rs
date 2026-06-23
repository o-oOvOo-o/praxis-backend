#![allow(warnings, clippy::all)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::fs::FileTimes;
use std::io::Write;
use std::path::Path;

use chrono::TimeZone;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use time::Duration;
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use uuid::Uuid;

use crate::INTERACTIVE_SESSION_SOURCES;
use crate::find_thread_path_by_id_str;
use crate::list::Cursor;
use crate::list::ThreadItem;
use crate::list::ThreadSortKey;
use crate::list::ThreadsPage;
use crate::list::get_threads;
use crate::list::read_head_for_summary;
use crate::rollout_date_parts;
use anyhow::Result;
use praxis_protocol::ThreadId;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::USER_MESSAGE_BEGIN;
use praxis_protocol::protocol::UserMessageEvent;

const NO_SOURCE_FILTER: &[SessionSource] = &[];
const TEST_PROVIDER: &str = "test-provider";

fn provider_vec(providers: &[&str]) -> Vec<String> {
    providers
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

fn thread_id_from_uuid(uuid: Uuid) -> ThreadId {
    ThreadId::from_string(&uuid.to_string()).expect("valid thread id")
}

async fn insert_state_db_thread(
    home: &Path,
    thread_id: ThreadId,
    rollout_path: &Path,
    archived: bool,
) {
    let runtime = praxis_state::StateRuntime::init(home.to_path_buf(), TEST_PROVIDER.to_string())
        .await
        .expect("state db should initialize");
    runtime
        .mark_backfill_complete(/*last_watermark*/ None)
        .await
        .expect("backfill should be complete");
    let created_at = chrono::Utc
        .with_ymd_and_hms(2025, 1, 3, 12, 0, 0)
        .single()
        .expect("valid datetime");
    let mut builder = praxis_state::ThreadMetadataBuilder::new(
        thread_id,
        rollout_path.to_path_buf(),
        created_at,
        SessionSource::Cli,
    );
    builder.model_provider = Some(TEST_PROVIDER.to_string());
    builder.cwd = home.to_path_buf();
    if archived {
        builder.archived_at = Some(created_at);
    }
    let mut metadata = builder.build(TEST_PROVIDER);
    metadata.first_user_message = Some("Hello from user".to_string());
    runtime
        .upsert_thread(&metadata)
        .await
        .expect("state db upsert should succeed");
}

// TODO(jif) fix
// #[tokio::test]
// async fn list_threads_prefers_state_db_when_available() {
//     let temp = TempDir::new().unwrap();
//     let home = temp.path();
//     let fs_uuid = Uuid::from_u128(101);
//     write_session_file(
//         home,
//         "2025-01-03T13-00-00",
//         fs_uuid,
//         1,
//         Some(SessionSource::Cli),
//     )
//     .unwrap();
//
//     let db_uuid = Uuid::from_u128(102);
//     let db_thread_id = ThreadId::from_string(&db_uuid.to_string()).expect("valid thread id");
//     let db_rollout_path = home.join(format!(
//         "sessions/2025/01/03/rollout-2025-01-03T12-00-00-{db_uuid}.jsonl"
//     ));
//     insert_state_db_thread(home, db_thread_id, db_rollout_path.as_path(), false).await;
//
//     let page = RolloutRecorder::list_threads(
//         home,
//         10,
//         None,
//         ThreadSortKey::CreatedAt,
//         NO_SOURCE_FILTER,
//         None,
//         TEST_PROVIDER,
//     )
//     .await
//     .expect("thread listing should succeed");
//
//     assert_eq!(page.items.len(), 1);
//     assert_eq!(page.items[0].path, db_rollout_path);
//     assert_eq!(page.items[0].thread_id, Some(db_thread_id));
//     assert_eq!(page.items[0].cwd, Some(home.to_path_buf()));
//     assert_eq!(
//         page.items[0].first_user_message.as_deref(),
//         Some("Hello from user")
//     );
// }

// #[tokio::test]
// async fn list_threads_db_excludes_archived_entries() {
//     let temp = TempDir::new().unwrap();
//     let home = temp.path();
//     let sessions_root = home.join("sessions/2025/01/03");
//     let archived_root = home.join("archived_sessions");
//     fs::create_dir_all(&sessions_root).unwrap();
//     fs::create_dir_all(&archived_root).unwrap();
//
//     let active_uuid = Uuid::from_u128(211);
//     let active_thread_id =
//         ThreadId::from_string(&active_uuid.to_string()).expect("valid active thread id");
//     let active_rollout_path =
//         sessions_root.join(format!("rollout-2025-01-03T12-00-00-{active_uuid}.jsonl"));
//     insert_state_db_thread(home, active_thread_id, active_rollout_path.as_path(), false).await;
//
//     let archived_uuid = Uuid::from_u128(212);
//     let archived_thread_id =
//         ThreadId::from_string(&archived_uuid.to_string()).expect("valid archived thread id");
//     let archived_rollout_path =
//         archived_root.join(format!("rollout-2025-01-03T11-00-00-{archived_uuid}.jsonl"));
//     insert_state_db_thread(
//         home,
//         archived_thread_id,
//         archived_rollout_path.as_path(),
//         true,
//     )
//     .await;
//
//     let page = RolloutRecorder::list_threads(
//         home,
//         10,
//         None,
//         ThreadSortKey::CreatedAt,
//         NO_SOURCE_FILTER,
//         None,
//         TEST_PROVIDER,
//     )
//     .await
//     .expect("thread listing should succeed");
//
//     assert_eq!(page.items.len(), 1);
//     assert_eq!(page.items[0].path, active_rollout_path);
// }

// #[tokio::test]
// async fn list_threads_falls_back_to_files_when_state_db_is_unavailable() {
//     let temp = TempDir::new().unwrap();
//     let home = temp.path();
//     let fs_uuid = Uuid::from_u128(301);
//     write_session_file(
//         home,
//         "2025-01-03T13-00-00",
//         fs_uuid,
//         1,
//         Some(SessionSource::Cli),
//     )
//     .unwrap();
//
//     let page = RolloutRecorder::list_threads(
//         home,
//         10,
//         None,
//         ThreadSortKey::CreatedAt,
//         NO_SOURCE_FILTER,
//         None,
//         TEST_PROVIDER,
//     )
//     .await
//     .expect("thread listing should succeed");
//
//     assert_eq!(page.items.len(), 1);
//     let file_name = page.items[0]
//         .path
//         .file_name()
//         .and_then(|value| value.to_str())
//         .expect("rollout file name should be utf8");
//     assert!(
//         file_name.contains(&fs_uuid.to_string()),
//         "expected file path from filesystem listing, got: {file_name}"
//     );
// }

#[tokio::test]
async fn find_thread_path_falls_back_when_db_path_is_stale() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let uuid = Uuid::from_u128(302);
    let thread_id = ThreadId::from_string(&uuid.to_string()).expect("valid thread id");
    let ts = "2025-01-03T13-00-00";
    write_session_file(
        home,
        ts,
        uuid,
        /*num_records*/ 1,
        Some(SessionSource::Cli),
    )
    .unwrap();
    let fs_rollout_path = home.join(format!("sessions/2025/01/03/rollout-{ts}-{uuid}.jsonl"));

    let stale_db_path = home.join(format!(
        "sessions/2099/01/01/rollout-2099-01-01T00-00-00-{uuid}.jsonl"
    ));
    insert_state_db_thread(
        home,
        thread_id,
        stale_db_path.as_path(),
        /*archived*/ false,
    )
    .await;

    let found = find_thread_path_by_id_str(home, &uuid.to_string())
        .await
        .expect("lookup should succeed");
    assert_eq!(found, Some(fs_rollout_path.clone()));
    assert_state_db_rollout_path(home, thread_id, Some(fs_rollout_path.as_path())).await;
}

#[tokio::test]
async fn find_thread_path_repairs_missing_db_row_after_filesystem_fallback() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let uuid = Uuid::from_u128(303);
    let thread_id = ThreadId::from_string(&uuid.to_string()).expect("valid thread id");
    let ts = "2025-01-03T13-00-00";
    write_session_file(
        home,
        ts,
        uuid,
        /*num_records*/ 1,
        Some(SessionSource::Cli),
    )
    .unwrap();
    let fs_rollout_path = home.join(format!("sessions/2025/01/03/rollout-{ts}-{uuid}.jsonl"));

    // Create an empty state DB so lookup takes the DB-first path and then falls back to files.
    let _runtime = praxis_state::StateRuntime::init(home.to_path_buf(), TEST_PROVIDER.to_string())
        .await
        .expect("state db should initialize");
    _runtime
        .mark_backfill_complete(/*last_watermark*/ None)
        .await
        .expect("backfill should be complete");

    let found = find_thread_path_by_id_str(home, &uuid.to_string())
        .await
        .expect("lookup should succeed");
    assert_eq!(found, Some(fs_rollout_path.clone()));
    assert_state_db_rollout_path(home, thread_id, Some(fs_rollout_path.as_path())).await;
}

#[test]
fn rollout_date_parts_extracts_directory_components() {
    let file_name = OsStr::new("rollout-2025-03-01T09-00-00-123.jsonl");
    let parts = rollout_date_parts(file_name);
    assert_eq!(
        parts,
        Some(("2025".to_string(), "03".to_string(), "01".to_string()))
    );
}

async fn assert_state_db_rollout_path(
    home: &Path,
    thread_id: ThreadId,
    expected_path: Option<&Path>,
) {
    let runtime = praxis_state::StateRuntime::init(home.to_path_buf(), TEST_PROVIDER.to_string())
        .await
        .expect("state db should initialize");
    let path = runtime
        .find_rollout_path_by_id(thread_id, Some(false))
        .await
        .expect("state db lookup should succeed");
    assert_eq!(path.as_deref(), expected_path);
}

fn write_session_file(
    root: &Path,
    ts_str: &str,
    uuid: Uuid,
    num_records: usize,
    source: Option<SessionSource>,
) -> std::io::Result<(OffsetDateTime, Uuid)> {
    write_session_file_with_provider(
        root,
        ts_str,
        uuid,
        num_records,
        source,
        Some("test-provider"),
    )
}

fn write_session_file_with_provider(
    root: &Path,
    ts_str: &str,
    uuid: Uuid,
    num_records: usize,
    source: Option<SessionSource>,
    model_provider: Option<&str>,
) -> std::io::Result<(OffsetDateTime, Uuid)> {
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let dt = PrimitiveDateTime::parse(ts_str, format)
        .unwrap()
        .assume_utc();
    let dir = root
        .join("sessions")
        .join(format!("{:04}", dt.year()))
        .join(format!("{:02}", u8::from(dt.month())))
        .join(format!("{:02}", dt.day()));
    fs::create_dir_all(&dir)?;

    let filename = format!("rollout-{ts_str}-{uuid}.jsonl");
    let file_path = dir.join(filename);
    let mut file = File::create(file_path)?;

    let mut payload = serde_json::json!({
        "id": uuid,
        "timestamp": ts_str,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version",
        "base_instructions": null,
    });

    if let Some(source) = source {
        payload["source"] = serde_json::to_value(source).unwrap();
    }
    if let Some(provider) = model_provider {
        payload["model_provider"] = serde_json::Value::String(provider.to_string());
    }

    let meta = serde_json::json!({
        "timestamp": ts_str,
        "type": "session_meta",
        "payload": payload,
    });
    writeln!(file, "{meta}")?;

    // Include at least one user message event to satisfy listing filters
    let user_event = serde_json::json!({
        "timestamp": ts_str,
        "type": "event_msg",
        "payload": {
            "type": "user_message",
            "message": "Hello from user",
            "kind": "plain"
        }
    });
    writeln!(file, "{user_event}")?;

    for i in 0..num_records {
        let rec = serde_json::json!({
            "record_type": "response",
            "index": i
        });
        writeln!(file, "{rec}")?;
    }
    let times = FileTimes::new().set_modified(dt.into());
    file.set_times(times)?;
    Ok((dt, uuid))
}

fn write_session_file_with_delayed_user_event(
    root: &Path,
    ts_str: &str,
    uuid: Uuid,
    meta_lines_before_user: usize,
) -> std::io::Result<()> {
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let dt = PrimitiveDateTime::parse(ts_str, format)
        .unwrap()
        .assume_utc();
    let dir = root
        .join("sessions")
        .join(format!("{:04}", dt.year()))
        .join(format!("{:02}", u8::from(dt.month())))
        .join(format!("{:02}", dt.day()));
    fs::create_dir_all(&dir)?;

    let filename = format!("rollout-{ts_str}-{uuid}.jsonl");
    let file_path = dir.join(filename);
    let mut file = File::create(file_path)?;

    for i in 0..meta_lines_before_user {
        let id = if i == 0 {
            uuid
        } else {
            Uuid::from_u128(100 + i as u128)
        };
        let payload = serde_json::json!({
            "id": id,
            "timestamp": ts_str,
            "cwd": ".",
            "originator": "test_originator",
            "cli_version": "test_version",
            "source": "vscode",
            "model_provider": "test-provider",
        });
        let meta = serde_json::json!({
            "timestamp": ts_str,
            "type": "session_meta",
            "payload": payload,
        });
        writeln!(file, "{meta}")?;
    }

    let user_event = serde_json::json!({
        "timestamp": ts_str,
        "type": "event_msg",
        "payload": {"type": "user_message", "message": "Hello from user", "kind": "plain"}
    });
    writeln!(file, "{user_event}")?;

    let times = FileTimes::new().set_modified(dt.into());
    file.set_times(times)?;
    Ok(())
}

fn write_session_file_with_meta_payload(
    root: &Path,
    ts_str: &str,
    uuid: Uuid,
    payload: serde_json::Value,
) -> std::io::Result<()> {
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let dt = PrimitiveDateTime::parse(ts_str, format)
        .unwrap()
        .assume_utc();
    let dir = root
        .join("sessions")
        .join(format!("{:04}", dt.year()))
        .join(format!("{:02}", u8::from(dt.month())))
        .join(format!("{:02}", dt.day()));
    fs::create_dir_all(&dir)?;

    let filename = format!("rollout-{ts_str}-{uuid}.jsonl");
    let file_path = dir.join(filename);
    let mut file = File::create(file_path)?;

    let meta = serde_json::json!({
        "timestamp": ts_str,
        "type": "session_meta",
        "payload": payload,
    });
    writeln!(file, "{meta}")?;

    let user_event = serde_json::json!({
        "timestamp": ts_str,
        "type": "event_msg",
        "payload": {"type": "user_message", "message": "Hello from user", "kind": "plain"}
    });
    writeln!(file, "{user_event}")?;

    let times = FileTimes::new().set_modified(dt.into());
    file.set_times(times)?;

    Ok(())
}

mod contents;
mod filters;
mod listing;
mod pagination;
mod timestamps;
mod user_events;
