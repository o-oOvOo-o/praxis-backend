use super::*;

#[tokio::test]
async fn test_list_conversations_latest_first() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    // Fixed UUIDs for deterministic expectations
    let u1 = Uuid::from_u128(1);
    let u2 = Uuid::from_u128(2);
    let u3 = Uuid::from_u128(3);

    // Create three sessions across three days
    write_session_file(
        home,
        "2025-01-01T12-00-00",
        u1,
        /*num_records*/ 3,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-01-02T12-00-00",
        u2,
        /*num_records*/ 3,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-01-03T12-00-00",
        u3,
        /*num_records*/ 3,
        Some(SessionSource::VSCode),
    )
    .unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();

    // Build expected objects
    let p1 = home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("03")
        .join(format!("rollout-2025-01-03T12-00-00-{u3}.jsonl"));
    let p2 = home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("02")
        .join(format!("rollout-2025-01-02T12-00-00-{u2}.jsonl"));
    let p3 = home
        .join("sessions")
        .join("2025")
        .join("01")
        .join("01")
        .join(format!("rollout-2025-01-01T12-00-00-{u1}.jsonl"));

    let updated_times: Vec<Option<String>> =
        page.items.iter().map(|i| i.updated_at.clone()).collect();

    let expected = ThreadsPage {
        items: vec![
            ThreadItem {
                path: p1,
                thread_id: Some(thread_id_from_uuid(u3)),
                first_user_message: Some("Hello from user".to_string()),
                cwd: Some(Path::new(".").to_path_buf()),
                git_branch: None,
                git_sha: None,
                git_origin_url: None,
                source: Some(SessionSource::VSCode),
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: Some(TEST_PROVIDER.to_string()),
                cli_version: Some("test_version".to_string()),
                created_at: Some("2025-01-03T12-00-00".into()),
                updated_at: updated_times.first().cloned().flatten(),
            },
            ThreadItem {
                path: p2,
                thread_id: Some(thread_id_from_uuid(u2)),
                first_user_message: Some("Hello from user".to_string()),
                cwd: Some(Path::new(".").to_path_buf()),
                git_branch: None,
                git_sha: None,
                git_origin_url: None,
                source: Some(SessionSource::VSCode),
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: Some(TEST_PROVIDER.to_string()),
                cli_version: Some("test_version".to_string()),
                created_at: Some("2025-01-02T12-00-00".into()),
                updated_at: updated_times.get(1).cloned().flatten(),
            },
            ThreadItem {
                path: p3,
                thread_id: Some(thread_id_from_uuid(u1)),
                first_user_message: Some("Hello from user".to_string()),
                cwd: Some(Path::new(".").to_path_buf()),
                git_branch: None,
                git_sha: None,
                git_origin_url: None,
                source: Some(SessionSource::VSCode),
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: Some(TEST_PROVIDER.to_string()),
                cli_version: Some("test_version".to_string()),
                created_at: Some("2025-01-01T12-00-00".into()),
                updated_at: updated_times.get(2).cloned().flatten(),
            },
        ],
        next_cursor: None,
        num_scanned_files: 3,
        reached_scan_cap: false,
    };

    assert_eq!(page, expected);
}

#[tokio::test]
async fn test_list_threads_scans_past_head_for_user_event() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let uuid = Uuid::from_u128(99);
    let ts = "2025-05-01T10-30-00";
    write_session_file_with_delayed_user_event(home, ts, uuid, /*meta_lines_before_user*/ 12)
        .unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].thread_id, Some(thread_id_from_uuid(uuid)));
}
