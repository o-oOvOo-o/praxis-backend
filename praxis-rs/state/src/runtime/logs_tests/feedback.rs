use super::*;

#[tokio::test]
async fn query_feedback_logs_returns_newest_lines_within_limit_in_order() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("alpha".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("bravo".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("charlie".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let bytes = runtime
        .query_feedback_logs("thread-1")
        .await
        .expect("query feedback logs");

    assert_eq!(
        String::from_utf8(bytes).expect("valid utf-8"),
        [
            format_feedback_log_line(/*ts*/ 1, /*ts_nanos*/ 0, "INFO", "alpha"),
            format_feedback_log_line(/*ts*/ 2, /*ts_nanos*/ 0, "INFO", "bravo"),
            format_feedback_log_line(/*ts*/ 3, /*ts_nanos*/ 0, "INFO", "charlie"),
        ]
        .concat()
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn query_feedback_logs_excludes_oversized_newest_row() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");
    let eleven_mebibytes = "z".repeat(11 * 1024 * 1024);

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("small".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-oversized".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(eleven_mebibytes),
                feedback_log_body: None,
                thread_id: Some("thread-oversized".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let bytes = runtime
        .query_feedback_logs("thread-oversized")
        .await
        .expect("query feedback logs");

    assert_eq!(bytes, Vec::<u8>::new());

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn query_feedback_logs_includes_threadless_rows_from_same_process() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("threadless-before".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("thread-scoped".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("threadless-after".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 4,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("other-process-threadless".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-2".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let bytes = runtime
        .query_feedback_logs("thread-1")
        .await
        .expect("query feedback logs");

    assert_eq!(
        String::from_utf8(bytes).expect("valid utf-8"),
        [
            format_feedback_log_line(
                /*ts*/ 1,
                /*ts_nanos*/ 0,
                "INFO",
                "threadless-before"
            ),
            format_feedback_log_line(/*ts*/ 2, /*ts_nanos*/ 0, "INFO", "thread-scoped"),
            format_feedback_log_line(
                /*ts*/ 3,
                /*ts_nanos*/ 0,
                "INFO",
                "threadless-after"
            ),
        ]
        .concat()
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn query_feedback_logs_excludes_threadless_rows_from_prior_processes() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("old-process-threadless".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-old".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("old-process-thread".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-old".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("new-process-thread".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-new".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 4,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("new-process-threadless".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-new".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let bytes = runtime
        .query_feedback_logs("thread-1")
        .await
        .expect("query feedback logs");

    assert_eq!(
        String::from_utf8(bytes).expect("valid utf-8"),
        [
            format_feedback_log_line(
                /*ts*/ 2,
                /*ts_nanos*/ 0,
                "INFO",
                "old-process-thread"
            ),
            format_feedback_log_line(
                /*ts*/ 3,
                /*ts_nanos*/ 0,
                "INFO",
                "new-process-thread"
            ),
            format_feedback_log_line(
                /*ts*/ 4,
                /*ts_nanos*/ 0,
                "INFO",
                "new-process-threadless"
            ),
        ]
        .concat()
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn query_feedback_logs_keeps_newest_suffix_across_thread_and_threadless_logs() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");
    let thread_marker = "thread-scoped-oldest";
    let threadless_older_marker = "threadless-older";
    let threadless_newer_marker = "threadless-newer";
    let five_mebibytes = format!("{threadless_older_marker} {}", "a".repeat(5 * 1024 * 1024));
    let four_and_half_mebibytes = format!(
        "{threadless_newer_marker} {}",
        "b".repeat((9 * 1024 * 1024) / 2)
    );
    let one_mebibyte = format!("{thread_marker} {}", "c".repeat(1024 * 1024));

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(one_mebibyte.clone()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(five_mebibytes),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some(four_and_half_mebibytes),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let bytes = runtime
        .query_feedback_logs("thread-1")
        .await
        .expect("query feedback logs");
    let logs = String::from_utf8(bytes).expect("valid utf-8");

    assert!(!logs.contains(thread_marker));
    assert!(logs.contains(threadless_older_marker));
    assert!(logs.contains(threadless_newer_marker));
    assert_eq!(logs.matches('\n').count(), 2);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn query_feedback_logs_for_threads_merges_requested_threads_and_threadless_rows() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    runtime
        .insert_logs(&[
            LogEntry {
                ts: 1,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("thread-1".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-1".to_string()),
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 2,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("thread-2".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-2".to_string()),
                process_uuid: Some("proc-2".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 3,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("threadless-proc-1".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-1".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 4,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("threadless-proc-2".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-2".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 5,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("thread-3".to_string()),
                feedback_log_body: None,
                thread_id: Some("thread-3".to_string()),
                process_uuid: Some("proc-3".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
            LogEntry {
                ts: 6,
                ts_nanos: 0,
                level: "INFO".to_string(),
                target: "cli".to_string(),
                message: Some("threadless-proc-3".to_string()),
                feedback_log_body: None,
                thread_id: None,
                process_uuid: Some("proc-3".to_string()),
                file: None,
                line: None,
                module_path: None,
            },
        ])
        .await
        .expect("insert test logs");

    let bytes = runtime
        .query_feedback_logs_for_threads(&["thread-1", "thread-2"])
        .await
        .expect("query feedback logs");

    assert_eq!(
        String::from_utf8(bytes).expect("valid utf-8"),
        [
            format_feedback_log_line(/*ts*/ 1, /*ts_nanos*/ 0, "INFO", "thread-1"),
            format_feedback_log_line(/*ts*/ 2, /*ts_nanos*/ 0, "INFO", "thread-2"),
            format_feedback_log_line(
                /*ts*/ 3,
                /*ts_nanos*/ 0,
                "INFO",
                "threadless-proc-1"
            ),
            format_feedback_log_line(
                /*ts*/ 4,
                /*ts_nanos*/ 0,
                "INFO",
                "threadless-proc-2"
            ),
        ]
        .concat()
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn query_feedback_logs_for_threads_returns_empty_for_empty_thread_list() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let bytes = runtime
        .query_feedback_logs_for_threads(&[])
        .await
        .expect("query feedback logs");

    assert_eq!(bytes, Vec::<u8>::new());

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}
