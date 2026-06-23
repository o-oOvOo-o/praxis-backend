use super::*;

#[tokio::test]
async fn test_pagination_cursor() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    // Fixed UUIDs for deterministic expectations
    let u1 = Uuid::from_u128(11);
    let u2 = Uuid::from_u128(22);
    let u3 = Uuid::from_u128(33);
    let u4 = Uuid::from_u128(44);
    let u5 = Uuid::from_u128(55);

    // Oldest to newest
    write_session_file(
        home,
        "2025-03-01T09-00-00",
        u1,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-03-02T09-00-00",
        u2,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-03-03T09-00-00",
        u3,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-03-04T09-00-00",
        u4,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-03-05T09-00-00",
        u5,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
    )
    .unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page1 = get_threads(
        home,
        /*page_size*/ 2,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();
    let p5 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("05")
        .join(format!("rollout-2025-03-05T09-00-00-{u5}.jsonl"));
    let p4 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("04")
        .join(format!("rollout-2025-03-04T09-00-00-{u4}.jsonl"));
    let updated_page1: Vec<Option<String>> =
        page1.items.iter().map(|i| i.updated_at.clone()).collect();
    let expected_cursor1: Cursor =
        serde_json::from_str(&format!("\"2025-03-04T09-00-00|{u4}\"")).unwrap();
    let expected_page1 = ThreadsPage {
        items: vec![
            ThreadItem {
                path: p5,
                thread_id: Some(thread_id_from_uuid(u5)),
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
                created_at: Some("2025-03-05T09-00-00".into()),
                updated_at: updated_page1.first().cloned().flatten(),
            },
            ThreadItem {
                path: p4,
                thread_id: Some(thread_id_from_uuid(u4)),
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
                created_at: Some("2025-03-04T09-00-00".into()),
                updated_at: updated_page1.get(1).cloned().flatten(),
            },
        ],
        next_cursor: Some(expected_cursor1.clone()),
        num_scanned_files: 3, // scanned 05, 04, and peeked at 03 before breaking
        reached_scan_cap: false,
    };
    assert_eq!(page1, expected_page1);

    let page2 = get_threads(
        home,
        /*page_size*/ 2,
        page1.next_cursor.as_ref(),
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();
    let p3 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("03")
        .join(format!("rollout-2025-03-03T09-00-00-{u3}.jsonl"));
    let p2 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("02")
        .join(format!("rollout-2025-03-02T09-00-00-{u2}.jsonl"));
    let updated_page2: Vec<Option<String>> =
        page2.items.iter().map(|i| i.updated_at.clone()).collect();
    let expected_cursor2: Cursor =
        serde_json::from_str(&format!("\"2025-03-02T09-00-00|{u2}\"")).unwrap();
    let expected_page2 = ThreadsPage {
        items: vec![
            ThreadItem {
                path: p3,
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
                created_at: Some("2025-03-03T09-00-00".into()),
                updated_at: updated_page2.first().cloned().flatten(),
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
                created_at: Some("2025-03-02T09-00-00".into()),
                updated_at: updated_page2.get(1).cloned().flatten(),
            },
        ],
        next_cursor: Some(expected_cursor2.clone()),
        num_scanned_files: 5, // scanned 05, 04 (anchor), 03, 02, and peeked at 01
        reached_scan_cap: false,
    };
    assert_eq!(page2, expected_page2);

    let page3 = get_threads(
        home,
        /*page_size*/ 2,
        page2.next_cursor.as_ref(),
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();
    let p1 = home
        .join("sessions")
        .join("2025")
        .join("03")
        .join("01")
        .join(format!("rollout-2025-03-01T09-00-00-{u1}.jsonl"));
    let updated_page3: Vec<Option<String>> =
        page3.items.iter().map(|i| i.updated_at.clone()).collect();
    let expected_page3 = ThreadsPage {
        items: vec![ThreadItem {
            path: p1,
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
            created_at: Some("2025-03-01T09-00-00".into()),
            updated_at: updated_page3.first().cloned().flatten(),
        }],
        next_cursor: None,
        num_scanned_files: 5, // scanned 05, 04 (anchor), 03, 02 (anchor), 01
        reached_scan_cap: false,
    };
    assert_eq!(page3, expected_page3);
}
