use super::*;

#[tokio::test]
async fn test_source_filter_excludes_non_matching_sessions() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let interactive_id = Uuid::from_u128(42);
    let non_interactive_id = Uuid::from_u128(77);

    write_session_file(
        home,
        "2025-08-02T10-00-00",
        interactive_id,
        /*num_records*/ 2,
        Some(SessionSource::Cli),
    )
    .unwrap();
    write_session_file(
        home,
        "2025-08-01T10-00-00",
        non_interactive_id,
        /*num_records*/ 2,
        Some(SessionSource::Exec),
    )
    .unwrap();

    let provider_filter = provider_vec(&[TEST_PROVIDER]);
    let interactive_only = get_threads(
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
    let paths: Vec<_> = interactive_only
        .items
        .iter()
        .map(|item| item.path.as_path())
        .collect();

    assert_eq!(paths.len(), 1);
    assert!(paths.iter().all(|path| {
        path.ends_with("rollout-2025-08-02T10-00-00-00000000-0000-0000-0000-00000000002a.jsonl")
    }));

    let all_sessions = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        NO_SOURCE_FILTER,
        /*model_providers*/ None,
        TEST_PROVIDER,
    )
    .await
    .unwrap();
    let all_paths: Vec<_> = all_sessions
        .items
        .into_iter()
        .map(|item| item.path)
        .collect();
    assert_eq!(all_paths.len(), 2);
    assert!(all_paths.iter().any(|path| {
        path.ends_with("rollout-2025-08-02T10-00-00-00000000-0000-0000-0000-00000000002a.jsonl")
    }));
    assert!(all_paths.iter().any(|path| {
        path.ends_with("rollout-2025-08-01T10-00-00-00000000-0000-0000-0000-00000000004d.jsonl")
    }));
}

#[tokio::test]
async fn test_model_provider_filter_selects_only_matching_sessions() -> Result<()> {
    let temp = TempDir::new().unwrap();
    let home = temp.path();

    let openai_id = Uuid::from_u128(1);
    let beta_id = Uuid::from_u128(2);
    let none_id = Uuid::from_u128(3);

    write_session_file_with_provider(
        home,
        "2025-09-01T12-00-00",
        openai_id,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
        Some("openai"),
    )?;
    write_session_file_with_provider(
        home,
        "2025-09-01T11-00-00",
        beta_id,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
        Some("beta"),
    )?;
    write_session_file_with_provider(
        home,
        "2025-09-01T10-00-00",
        none_id,
        /*num_records*/ 1,
        Some(SessionSource::VSCode),
        /*model_provider*/ None,
    )?;

    let openai_id_str = openai_id.to_string();
    let none_id_str = none_id.to_string();
    let openai_filter = provider_vec(&["openai"]);
    let openai_sessions = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        NO_SOURCE_FILTER,
        Some(openai_filter.as_slice()),
        "openai",
    )
    .await?;
    assert_eq!(openai_sessions.items.len(), 2);
    let openai_ids: Vec<_> = openai_sessions
        .items
        .iter()
        .filter_map(|item| item.thread_id.as_ref().map(ToString::to_string))
        .collect();
    assert!(openai_ids.contains(&openai_id_str));
    assert!(openai_ids.contains(&none_id_str));

    let beta_filter = provider_vec(&["beta"]);
    let beta_sessions = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        NO_SOURCE_FILTER,
        Some(beta_filter.as_slice()),
        "openai",
    )
    .await?;
    assert_eq!(beta_sessions.items.len(), 1);
    let beta_id_str = beta_id.to_string();
    let beta_head = beta_sessions
        .items
        .first()
        .and_then(|item| item.thread_id.as_ref().map(ToString::to_string));
    assert_eq!(beta_head.as_deref(), Some(beta_id_str.as_str()));

    let unknown_filter = provider_vec(&["unknown"]);
    let unknown_sessions = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        NO_SOURCE_FILTER,
        Some(unknown_filter.as_slice()),
        "openai",
    )
    .await?;
    assert!(unknown_sessions.items.is_empty());

    let all_sessions = get_threads(
        home,
        /*page_size*/ 10,
        /*cursor*/ None,
        ThreadSortKey::CreatedAt,
        NO_SOURCE_FILTER,
        /*model_providers*/ None,
        "openai",
    )
    .await?;
    assert_eq!(all_sessions.items.len(), 3);

    Ok(())
}
