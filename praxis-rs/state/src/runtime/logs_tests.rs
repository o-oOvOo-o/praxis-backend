use super::StateRuntime;
use super::format_feedback_log_line;
use super::test_support::unique_temp_dir;
use crate::LogEntry;
use crate::LogQuery;
use crate::logs_db_path;
use pretty_assertions::assert_eq;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::path::Path;

async fn open_db_pool(path: &Path) -> SqlitePool {
    SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false),
    )
    .await
    .expect("open sqlite pool")
}

async fn log_row_count(path: &Path) -> i64 {
    let pool = open_db_pool(path).await;
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM logs")
        .fetch_one(&pool)
        .await
        .expect("count log rows");
    pool.close().await;
    count
}

#[tokio::test]
async fn insert_logs_use_dedicated_log_database() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    runtime
        .insert_logs(&[LogEntry {
            ts: 1,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some("dedicated-log-db".to_string()),
            feedback_log_body: Some("dedicated-log-db".to_string()),
            thread_id: Some("thread-1".to_string()),
            process_uuid: Some("proc-1".to_string()),
            module_path: Some("mod".to_string()),
            file: Some("main.rs".to_string()),
            line: Some(7),
        }])
        .await
        .expect("insert test logs");

    let logs_count = log_row_count(logs_db_path(praxis_home.as_path()).as_path()).await;

    assert_eq!(logs_count, 1);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn init_configures_logs_db_with_incremental_auto_vacuum() {
    let praxis_home = unique_temp_dir();
    let _runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let pool = open_db_pool(logs_db_path(praxis_home.as_path()).as_path()).await;
    let auto_vacuum = sqlx::query_scalar::<_, i64>("PRAGMA auto_vacuum")
        .fetch_one(&pool)
        .await
        .expect("read auto_vacuum pragma");
    assert_eq!(auto_vacuum, 2);
    pool.close().await;

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[test]
fn format_feedback_log_line_matches_feedback_formatter_shape() {
    assert_eq!(
        format_feedback_log_line(
            /*ts*/ 1,
            /*ts_nanos*/ 123_456_000,
            "INFO",
            "alpha"
        ),
        "1970-01-01T00:00:01.123456Z  INFO alpha\n"
    );
}

#[test]
fn format_feedback_log_line_preserves_existing_trailing_newline() {
    assert_eq!(
        format_feedback_log_line(
            /*ts*/ 1,
            /*ts_nanos*/ 123_456_000,
            "INFO",
            "alpha\n"
        ),
        "1970-01-01T00:00:01.123456Z  INFO alpha\n"
    );
}

#[tokio::test]
async fn query_logs_with_search_matches_rendered_body_substring() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1_700_000_001,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("alpha".to_string()),
                feedback_log_body: Some("foo=1 alpha".to_string()),
                thread_id: Some("thread-1".to_string()),
                process_uuid: None,
                file: Some("main.rs".to_string()),
                line: Some(42),
                module_path: None,
            },
            LogEntry {
                ts: 1_700_000_002,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("alphabet".to_string()),
                feedback_log_body: Some("foo=2 alphabet".to_string()),
                thread_id: Some("thread-1".to_string()),
                process_uuid: None,
                file: Some("main.rs".to_string()),
                line: Some(43),
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            search: Some("foo=2".to_string()),
            ..Default::default()
        })
        .await
        .expect("query matching logs");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.as_deref(), Some("foo=2 alphabet"));

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_old_rows_when_thread_exceeds_size_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let six_mebibytes = "a".repeat(6 * 1024 * 1024);
    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("small".to_string()),
                feedback_log_body: Some(six_mebibytes.clone()),
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: Some("main.rs".to_string()),
                line: Some(1),
                module_path: Some("mod".to_string()),
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("small".to_string()),
                feedback_log_body: Some(six_mebibytes.clone()),
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: Some("main.rs".to_string()),
                line: Some(2),
                module_path: Some("mod".to_string()),
            },
        ])
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            thread_ids: vec!["thread-1".to_string()],
            ..Default::default()
        })
        .await
        .expect("query thread logs");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ts, 2);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_single_thread_row_when_it_exceeds_size_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let eleven_mebibytes = "d".repeat(11 * 1024 * 1024);
    runtime
        .insert_logs(&[LogEntry {
            ts: 1,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some("small".to_string()),
            feedback_log_body: Some(eleven_mebibytes),
            thread_id: Some("thread-oversized".to_string()),
            process_uuid: Some("proc-1".to_string()),
            file: Some("main.rs".to_string()),
            line: Some(1),
            module_path: Some("mod".to_string()),
        }])
        .await
        .expect("insert test log");

    let rows = runtime
        .query_logs(&LogQuery {
            thread_ids: vec!["thread-oversized".to_string()],
            ..Default::default()
        })
        .await
        .expect("query thread logs");

    assert!(rows.is_empty());

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_threadless_rows_per_process_uuid_only() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let six_mebibytes = "b".repeat(6 * 1024 * 1024);
    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(six_mebibytes.clone()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: Some("main.rs".to_string()),
                line: Some(1),
                module_path: Some("mod".to_string()),
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(six_mebibytes.clone()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: Some("main.rs".to_string()),
                line: Some(2),
                module_path: Some("mod".to_string()),
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(six_mebibytes),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: Some("main.rs".to_string()),
                line: Some(3),
                module_path: Some("mod".to_string()),
            },
        ])
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            thread_ids: vec!["thread-1".to_string()],
            include_threadless: true,
            ..Default::default()
        })
        .await
        .expect("query thread and threadless logs");

    let mut timestamps: Vec<i64> = rows.into_iter().map(|row| row.ts).collect();
    timestamps.sort_unstable();
    assert_eq!(timestamps, vec![2, 3]);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_single_threadless_process_row_when_it_exceeds_size_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let eleven_mebibytes = "e".repeat(11 * 1024 * 1024);
    runtime
        .insert_logs(&[LogEntry {
            ts: 1,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some("small".to_string()),
            feedback_log_body: Some(eleven_mebibytes),
            thread_id: None,
            process_uuid: Some("proc-oversized".to_string()),
            file: Some("main.rs".to_string()),
            line: Some(1),
            module_path: Some("mod".to_string()),
        }])
        .await
        .expect("insert test log");

    let rows = runtime
        .query_logs(&LogQuery {
            include_threadless: true,
            ..Default::default()
        })
        .await
        .expect("query threadless logs");

    assert!(rows.is_empty());

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_threadless_rows_with_null_process_uuid() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let six_mebibytes = "c".repeat(6 * 1024 * 1024);
    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(six_mebibytes.clone()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: None,
                file: Some("main.rs".to_string()),
                line: Some(1),
                module_path: Some("mod".to_string()),
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(six_mebibytes),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: None,
                file: Some("main.rs".to_string()),
                line: Some(2),
                module_path: Some("mod".to_string()),
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("small".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: Some("main.rs".to_string()),
                line: Some(3),
                module_path: Some("mod".to_string()),
            },
        ])
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            include_threadless: true,
            ..Default::default()
        })
        .await
        .expect("query threadless logs");

    let mut timestamps: Vec<i64> = rows.into_iter().map(|row| row.ts).collect();
    timestamps.sort_unstable();
    assert_eq!(timestamps, vec![2, 3]);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_single_threadless_null_process_row_when_it_exceeds_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let eleven_mebibytes = "f".repeat(11 * 1024 * 1024);
    runtime
        .insert_logs(&[LogEntry {
            ts: 1,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some("small".to_string()),
            feedback_log_body: Some(eleven_mebibytes),
            thread_id: None,
            process_uuid: None,
            file: Some("main.rs".to_string()),
            line: Some(1),
            module_path: Some("mod".to_string()),
        }])
        .await
        .expect("insert test log");

    let rows = runtime
        .query_logs(&LogQuery {
            include_threadless: true,
            ..Default::default()
        })
        .await
        .expect("query threadless logs");

    assert!(rows.is_empty());

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_old_rows_when_thread_exceeds_row_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let entries: Vec<LogEntry> = (1..=1_001)
        .map(|ts| LogEntry {
            ts,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some(format!("thread-row-{ts}")),
            feedback_log_body: None,
            thread_id: Some("thread-row-limit".to_string()),
            process_uuid: Some("proc-1".to_string()),
            file: Some("main.rs".to_string()),
            line: Some(ts),
            module_path: Some("mod".to_string()),
        })
        .collect();
    runtime
        .insert_logs(&entries)
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            thread_ids: vec!["thread-row-limit".to_string()],
            ..Default::default()
        })
        .await
        .expect("query thread logs");

    let timestamps: Vec<i64> = rows.into_iter().map(|row| row.ts).collect();
    assert_eq!(timestamps.len(), 1_000);
    assert_eq!(timestamps.first().copied(), Some(2));
    assert_eq!(timestamps.last().copied(), Some(1_001));

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_old_threadless_rows_when_process_exceeds_row_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let entries: Vec<LogEntry> = (1..=1_001)
        .map(|ts| LogEntry {
            ts,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some(format!("process-row-{ts}")),
            feedback_log_body: None,
            thread_id: None,
            process_uuid: Some("proc-row-limit".to_string()),
            file: Some("main.rs".to_string()),
            line: Some(ts),
            module_path: Some("mod".to_string()),
        })
        .collect();
    runtime
        .insert_logs(&entries)
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            include_threadless: true,
            ..Default::default()
        })
        .await
        .expect("query threadless logs");

    let timestamps: Vec<i64> = rows
        .into_iter()
        .filter(|row| row.process_uuid.as_deref() == Some("proc-row-limit"))
        .map(|row| row.ts)
        .collect();
    assert_eq!(timestamps.len(), 1_000);
    assert_eq!(timestamps.first().copied(), Some(2));
    assert_eq!(timestamps.last().copied(), Some(1_001));

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn insert_logs_prunes_old_threadless_null_process_rows_when_row_limit_exceeded() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let entries: Vec<LogEntry> = (1..=1_001)
        .map(|ts| LogEntry {
            ts,
            ts_nanos: 0,
            level: "INFO".to_string(),
            target: "cli".to_string(),
            message: Some(format!("null-process-row-{ts}")),
            feedback_log_body: None,
            thread_id: None,
            process_uuid: None,
            file: Some("main.rs".to_string()),
            line: Some(ts),
            module_path: Some("mod".to_string()),
        })
        .collect();
    runtime
        .insert_logs(&entries)
        .await
        .expect("insert test logs");

    let rows = runtime
        .query_logs(&LogQuery {
            include_threadless: true,
            ..Default::default()
        })
        .await
        .expect("query threadless logs");

    let timestamps: Vec<i64> = rows
        .into_iter()
        .filter(|row| row.process_uuid.is_none())
        .map(|row| row.ts)
        .collect();
    assert_eq!(timestamps.len(), 1_000);
    assert_eq!(timestamps.first().copied(), Some(2));
    assert_eq!(timestamps.last().copied(), Some(1_001));

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[path = "logs_tests/feedback.rs"]
mod feedback;
