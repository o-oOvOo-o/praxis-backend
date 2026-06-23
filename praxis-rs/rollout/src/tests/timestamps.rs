use super::*;

#[tokio::test]
async fn test_created_at_sort_uses_file_mtime_for_updated_at() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-06-01T08-00-00";
    let uuid = Uuid::from_u128(43);
    write_session_file(
        home,
        ts,
        uuid,
        /*num_records*/ 0,
        Some(SessionSource::VSCode),
    )
    .unwrap();

    let created = PrimitiveDateTime::parse(
        ts,
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]"),
    )?
    .assume_utc();
    let updated = created + Duration::hours(2);
    let expected_updated = updated.format(&time::format_description::well_known::Rfc3339)?;

    let file_path = home
        .join("sessions")
        .join("2025")
        .join("06")
        .join("01")
        .join(format!("rollout-{ts}-{uuid}.jsonl"));
    let file = std::fs::OpenOptions::new().write(true).open(&file_path)?;
    let times = FileTimes::new().set_modified(updated.into());
    file.set_times(times)?;

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
    .await?;

    let item = page.items.first().expect("conversation item");
    assert_eq!(item.created_at.as_deref(), Some(ts));
    assert_eq!(item.updated_at.as_deref(), Some(expected_updated.as_str()));

    Ok(())
}

#[tokio::test]
async fn test_updated_at_uses_file_mtime() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-06-01T08-00-00";
    let uuid = Uuid::from_u128(42);
    let day_dir = home.join("sessions").join("2025").join("06").join("01");
    fs::create_dir_all(&day_dir)?;
    let file_path = day_dir.join(format!("rollout-{ts}-{uuid}.jsonl"));
    let mut file = File::create(&file_path)?;

    let conversation_id = ThreadId::from_string(&uuid.to_string())?;
    let meta_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::SessionMeta(SessionMetaLine {
            meta: SessionMeta {
                id: conversation_id,
                forked_from_id: None,
                timestamp: ts.to_string(),
                cwd: ".".into(),
                originator: "test_originator".into(),
                cli_version: "test_version".into(),
                source: SessionSource::VSCode,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                model_provider: Some("test-provider".into()),
                base_instructions: None,
                dynamic_tools: None,
                memory_mode: None,
            },
            git: None,
        }),
    };
    writeln!(file, "{}", serde_json::to_string(&meta_line)?)?;

    let user_event_line = RolloutLine {
        timestamp: ts.to_string(),
        item: RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: "hello".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        })),
    };
    writeln!(file, "{}", serde_json::to_string(&user_event_line)?)?;

    let total_messages = 12usize;
    for idx in 0..total_messages {
        let response_line = RolloutLine {
            timestamp: format!("{ts}-{idx:02}"),
            item: RolloutItem::ResponseItem(ResponseItem::Message {
                id: None,
                role: "assistant".into(),
                content: vec![ContentItem::OutputText {
                    text: format!("reply-{idx}"),
                }],
                end_turn: None,
                phase: None,
            }),
        };
        writeln!(file, "{}", serde_json::to_string(&response_line)?)?;
    }
    drop(file);

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let page = get_threads(
        home,
        /*page_size*/ 1,
        /*cursor*/ None,
        ThreadSortKey::UpdatedAt,
        INTERACTIVE_SESSION_SOURCES.as_slice(),
        Some(provider_filter.as_slice()),
        TEST_PROVIDER,
    )
    .await?;
    let item = page.items.first().expect("conversation item");
    assert_eq!(item.created_at.as_deref(), Some(ts));
    let updated = item
        .updated_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .expect("updated_at set from file mtime");
    let now = chrono::Utc::now();
    let age = now - updated;
    assert!(age.num_seconds().abs() < 30);

    Ok(())
}

#[tokio::test]
async fn test_stable_ordering_same_second_pagination() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let ts = "2025-07-01T00-00-00";
    let u1 = Uuid::from_u128(1);
    let u2 = Uuid::from_u128(2);
    let u3 = Uuid::from_u128(3);

    write_session_file(
        home,
        ts,
        u1,
        /*num_records*/ 0,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        ts,
        u2,
        /*num_records*/ 0,
        Some(SessionSource::VSCode),
    )
    .unwrap();
    write_session_file(
        home,
        ts,
        u3,
        /*num_records*/ 0,
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

    let p3 = home
        .join("sessions")
        .join("2025")
        .join("07")
        .join("01")
        .join(format!("rollout-2025-07-01T00-00-00-{u3}.jsonl"));
    let p2 = home
        .join("sessions")
        .join("2025")
        .join("07")
        .join("01")
        .join(format!("rollout-2025-07-01T00-00-00-{u2}.jsonl"));
    let updated_page1: Vec<Option<String>> =
        page1.items.iter().map(|i| i.updated_at.clone()).collect();
    let expected_cursor1: Cursor = serde_json::from_str(&format!("\"{ts}|{u2}\"")).unwrap();
    let expected_page1 = ThreadsPage {
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
                created_at: Some(ts.to_string()),
                updated_at: updated_page1.first().cloned().flatten(),
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
                created_at: Some(ts.to_string()),
                updated_at: updated_page1.get(1).cloned().flatten(),
            },
        ],
        next_cursor: Some(expected_cursor1.clone()),
        num_scanned_files: 3, // scanned u3, u2, peeked u1
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
    let p1 = home
        .join("sessions")
        .join("2025")
        .join("07")
        .join("01")
        .join(format!("rollout-2025-07-01T00-00-00-{u1}.jsonl"));
    let updated_page2: Vec<Option<String>> =
        page2.items.iter().map(|i| i.updated_at.clone()).collect();
    let expected_page2 = ThreadsPage {
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
            created_at: Some(ts.to_string()),
            updated_at: updated_page2.first().cloned().flatten(),
        }],
        next_cursor: None,
        num_scanned_files: 3, // scanned u3, u2 (anchor), u1
        reached_scan_cap: false,
    };
    assert_eq!(page2, expected_page2);
}
