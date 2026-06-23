use super::*;

#[tokio::test]
async fn test_get_thread_contents() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let uuid = Uuid::new_v4();
    let ts = "2025-04-01T10-30-00";
    write_session_file(
        home,
        ts,
        uuid,
        /*num_records*/ 2,
        Some(SessionSource::VSCode),
    )
    .unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 1,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();
    let path = &page.items[0].path;

    let content = tokio::fs::read_to_string(path).await.unwrap();

    // Page equality (single item)
    let expected_path = home
        .join("sessions")
        .join("2025")
        .join("04")
        .join("01")
        .join(format!("rollout-2025-04-01T10-30-00-{uuid}.jsonl"));
    let expected_page = ThreadsPage {
        items: vec![ThreadItem {
            path: expected_path,
            thread_id: Some(thread_id_from_uuid(uuid)),
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
            created_at: Some(ts.into()),
            updated_at: page.items[0].updated_at.clone(),
        }],
        next_cursor: None,
        num_scanned_files: 1,
        reached_scan_cap: false,
    };
    assert_eq!(page, expected_page);

    // Entire file contents equality
    let meta = serde_json::json!({
        "timestamp": ts,
        "type": "session_meta",
        "payload": {
            "id": uuid,
            "timestamp": ts,
            "cwd": ".",
            "originator": "test_originator",
            "cli_version": "test_version",
            "base_instructions": null,
            "source": "vscode",
            "model_provider": "test-provider",
        }
    });
    let user_event = serde_json::json!({
        "timestamp": ts,
        "type": "event_msg",
        "payload": {"type": "user_message", "message": "Hello from user", "kind": "plain"}
    });
    let rec0 = serde_json::json!({"record_type": "response", "index": 0});
    let rec1 = serde_json::json!({"record_type": "response", "index": 1});
    let expected_content = format!("{meta}\n{user_event}\n{rec0}\n{rec1}\n");
    assert_eq!(content, expected_content);
}

#[tokio::test]
async fn test_base_instructions_missing_in_meta_defaults_to_null() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-04-02T10-30-00";
    let uuid = Uuid::from_u128(101);
    let payload = serde_json::json!({
        "id": uuid,
        "timestamp": ts,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version",
        "source": "vscode",
        "model_provider": "test-provider",
    });
    write_session_file_with_meta_payload(home, ts, uuid, payload).unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 1,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();

    let head = read_head_for_summary(&page.items[0].path)
        .await
        .expect("session meta head");
    let first = head.first().expect("first head entry");
    assert_eq!(
        first.get("base_instructions"),
        Some(&serde_json::Value::Null)
    );
}

#[tokio::test]
async fn test_base_instructions_present_in_meta_is_preserved() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-04-03T10-30-00";
    let uuid = Uuid::from_u128(102);
    let base_text = "Custom base instructions";
    let payload = serde_json::json!({
        "id": uuid,
        "timestamp": ts,
        "cwd": ".",
        "originator": "test_originator",
        "cli_version": "test_version",
        "source": "vscode",
        "model_provider": "test-provider",
        "base_instructions": {"text": base_text},
    });
    write_session_file_with_meta_payload(home, ts, uuid, payload).unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 1,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await
    .unwrap();

    let head = read_head_for_summary(&page.items[0].path)
        .await
        .expect("session meta head");
    let first = head.first().expect("first head entry");
    let base = first
        .get("base_instructions")
        .and_then(|value| value.get("text"))
        .and_then(serde_json::Value::as_str);
    assert_eq!(base, Some(base_text));
}
