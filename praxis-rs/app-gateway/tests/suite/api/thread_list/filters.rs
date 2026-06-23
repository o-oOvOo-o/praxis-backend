use super::*;

#[tokio::test]
async fn thread_list_pagination_next_cursor_none_on_last_page() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    // Create three rollouts so we can paginate with limit=2.
    let _a = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T12-00-00",
        "2025-01-02T12:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let _b = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T13-00-00",
        "2025-01-01T13:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let _c = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T12-00-00",
        "2025-01-01T12:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    // Page 1: limit 2 → expect next_cursor Some.
    let ThreadListResponse {
        data: data1,
        next_cursor: cursor1,
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(2),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert_eq!(data1.len(), 2);
    for thread in &data1 {
        assert_eq!(thread.preview, "Hello");
        assert_eq!(thread.model_provider, "mock_provider");
        assert!(thread.created_at > 0);
        assert_eq!(thread.updated_at, thread.created_at);
        assert_eq!(thread.cwd, PathBuf::from("/"));
        assert_eq!(thread.cli_version, "0.0.0");
        assert_eq!(thread.source, SessionSource::Cli);
        assert_eq!(thread.git_info, None);
        assert_eq!(thread.status, ThreadStatus::NotLoaded);
    }
    let cursor1 = cursor1.expect("expected nextCursor on first page");

    // Page 2: with cursor → expect next_cursor None when no more results.
    let ThreadListResponse {
        data: data2,
        next_cursor: cursor2,
    } = list_threads(
        &mut mcp,
        Some(cursor1),
        Some(2),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert!(data2.len() <= 2);
    for thread in &data2 {
        assert_eq!(thread.preview, "Hello");
        assert_eq!(thread.model_provider, "mock_provider");
        assert!(thread.created_at > 0);
        assert_eq!(thread.updated_at, thread.created_at);
        assert_eq!(thread.cwd, PathBuf::from("/"));
        assert_eq!(thread.cli_version, "0.0.0");
        assert_eq!(thread.source, SessionSource::Cli);
        assert_eq!(thread.git_info, None);
        assert_eq!(thread.status, ThreadStatus::NotLoaded);
    }
    assert_eq!(cursor2, None, "expected nextCursor to be null on last page");

    Ok(())
}

#[tokio::test]
async fn thread_list_respects_provider_filter() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    // Create rollouts under two providers.
    let _a = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T10-00-00",
        "2025-01-02T10:00:00Z",
        "X",
        Some("mock_provider"),
        /*git_info*/ None,
    )?; // mock_provider
    let _b = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T11-00-00",
        "2025-01-02T11:00:00Z",
        "X",
        Some("other_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    // Filter to only other_provider; expect 1 item, nextCursor None.
    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["other_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert_eq!(data.len(), 1);
    assert_eq!(next_cursor, None);
    let thread = &data[0];
    assert_eq!(thread.preview, "X");
    assert_eq!(thread.model_provider, "other_provider");
    let expected_ts = chrono::DateTime::parse_from_rfc3339("2025-01-02T11:00:00Z")?.timestamp();
    assert_eq!(thread.created_at, expected_ts);
    assert_eq!(thread.updated_at, expected_ts);
    assert_eq!(thread.cwd, PathBuf::from("/"));
    assert_eq!(thread.cli_version, "0.0.0");
    assert_eq!(thread.source, SessionSource::Cli);
    assert_eq!(thread.git_info, None);

    Ok(())
}

#[tokio::test]
async fn thread_list_respects_cwd_filter() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let filtered_id = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T10-00-00",
        "2025-01-02T10:00:00Z",
        "filtered",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let unfiltered_id = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T11-00-00",
        "2025-01-02T11:00:00Z",
        "unfiltered",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let target_cwd = praxis_home.path().join("target-cwd");
    fs::create_dir_all(&target_cwd)?;
    set_rollout_cwd(
        rollout_path(praxis_home.path(), "2025-01-02T10-00-00", &filtered_id).as_path(),
        &target_cwd,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;
    let request_id = mcp
        .send_thread_list_request(praxis_app_gateway_protocol::ThreadListParams {
            cursor: None,
            limit: Some(10),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: None,
            cwd: Some(target_cwd.to_string_lossy().into_owned()),
            cwd_scope: None,
            search_term: None,
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let ThreadListResponse {
        data, next_cursor, ..
    } = to_response::<ThreadListResponse>(resp)?;

    assert_eq!(next_cursor, None);
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].id, filtered_id);
    assert_ne!(data[0].id, unfiltered_id);
    assert_eq!(data[0].cwd, target_cwd);

    Ok(())
}

#[tokio::test]
async fn thread_list_respects_search_term_filter() -> Result<()> {
    let praxis_home = TempDir::new()?;
    std::fs::write(
        praxis_home.path().join("config.toml"),
        r#"
model = "mock-model"
approval_policy = "never"
suppress_unstable_features_warning = true

[features]
sqlite = true
"#,
    )?;

    let older_match = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T10-00-00",
        "2025-01-02T10:00:00Z",
        "match: needle",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let _non_match = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T11-00-00",
        "2025-01-02T11:00:00Z",
        "no hit here",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let newer_match = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T12-00-00",
        "2025-01-02T12:00:00Z",
        "needle suffix",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    // `thread/list` only applies `search_term` on the sqlite path. In this test we
    // create rollouts manually, so we must also create the sqlite DB and mark backfill
    // complete; otherwise app-gateway will permanently use filesystem fallback.
    let state_db =
        praxis_state::StateRuntime::init(praxis_home.path().to_path_buf(), "mock_provider".into())
            .await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;

    let mut mcp = init_mcp(praxis_home.path()).await?;
    let request_id = mcp
        .send_thread_list_request(praxis_app_gateway_protocol::ThreadListParams {
            cursor: None,
            limit: Some(10),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: None,
            cwd: None,
            cwd_scope: None,
            search_term: Some("needle".to_string()),
        })
        .await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let ThreadListResponse {
        data, next_cursor, ..
    } = to_response::<ThreadListResponse>(resp)?;

    assert_eq!(next_cursor, None);
    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids, vec![newer_match, older_match]);

    Ok(())
}
