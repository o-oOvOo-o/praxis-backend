use super::*;

#[tokio::test]
async fn thread_list_includes_git_info() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let git_info = CoreGitInfo {
        commit_hash: Some(GitSha::new("abc123")),
        branch: Some("main".to_string()),
        repository_url: Some("https://example.com/repo.git".to_string()),
    };
    let conversation_id = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T09-00-00",
        "2025-02-01T09:00:00Z",
        "Git info preview",
        Some("mock_provider"),
        Some(git_info),
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    let thread = data
        .iter()
        .find(|t| t.id == conversation_id)
        .expect("expected thread for created rollout");

    let expected_git = ApiGitInfo {
        sha: Some("abc123".to_string()),
        branch: Some("main".to_string()),
        origin_url: Some("https://example.com/repo.git".to_string()),
    };
    assert_eq!(thread.git_info, Some(expected_git));
    assert_eq!(thread.source, SessionSource::Cli);
    assert_eq!(thread.cwd, PathBuf::from("/"));
    assert_eq!(thread.cli_version, "0.0.0");

    Ok(())
}

#[tokio::test]
async fn thread_list_default_sorts_by_created_at() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let id_a = create_fake_rollout(
        praxis_home.path(),
        "2025-01-02T12-00-00",
        "2025-01-02T12:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_b = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T13-00-00",
        "2025-01-01T13:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_c = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T12-00-00",
        "2025-01-01T12:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads_with_sort(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*sort_key*/ None,
        /*archived*/ None,
    )
    .await?;

    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids, vec![id_a.as_str(), id_b.as_str(), id_c.as_str()]);

    Ok(())
}

#[tokio::test]
async fn thread_list_sort_updated_at_orders_by_mtime() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let id_old = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T10-00-00",
        "2025-01-01T10:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_mid = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T11-00-00",
        "2025-01-01T11:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_new = create_fake_rollout(
        praxis_home.path(),
        "2025-01-01T12-00-00",
        "2025-01-01T12:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-01-01T10-00-00", &id_old).as_path(),
        "2025-01-03T00:00:00Z",
    )?;
    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-01-01T11-00-00", &id_mid).as_path(),
        "2025-01-02T00:00:00Z",
    )?;
    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-01-01T12-00-00", &id_new).as_path(),
        "2025-01-01T00:00:00Z",
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads_with_sort(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        Some(ThreadSortKey::UpdatedAt),
        /*archived*/ None,
    )
    .await?;

    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids, vec![id_old.as_str(), id_mid.as_str(), id_new.as_str()]);

    Ok(())
}

#[tokio::test]
async fn thread_list_updated_at_paginates_with_cursor() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let id_a = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_b = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T11-00-00",
        "2025-02-01T11:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_c = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T12-00-00",
        "2025-02-01T12:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-02-01T10-00-00", &id_a).as_path(),
        "2025-02-03T00:00:00Z",
    )?;
    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-02-01T11-00-00", &id_b).as_path(),
        "2025-02-02T00:00:00Z",
    )?;
    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-02-01T12-00-00", &id_c).as_path(),
        "2025-02-01T00:00:00Z",
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse {
        data: page1,
        next_cursor: cursor1,
        ..
    } = list_threads_with_sort(
        &mut mcp,
        /*cursor*/ None,
        Some(2),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        Some(ThreadSortKey::UpdatedAt),
        /*archived*/ None,
    )
    .await?;
    let ids_page1: Vec<_> = page1.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids_page1, vec![id_a.as_str(), id_b.as_str()]);
    let cursor1 = cursor1.expect("expected nextCursor on first page");

    let ThreadListResponse {
        data: page2,
        next_cursor: cursor2,
        ..
    } = list_threads_with_sort(
        &mut mcp,
        Some(cursor1),
        Some(2),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        Some(ThreadSortKey::UpdatedAt),
        /*archived*/ None,
    )
    .await?;
    let ids_page2: Vec<_> = page2.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids_page2, vec![id_c.as_str()]);
    assert_eq!(cursor2, None);

    Ok(())
}

#[tokio::test]
async fn thread_list_created_at_tie_breaks_by_uuid() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let id_a = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_b = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;

    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    let mut expected = [id_a, id_b];
    expected.sort_by_key(|id| Reverse(Uuid::parse_str(id).expect("uuid should parse")));
    let expected: Vec<_> = expected.iter().map(String::as_str).collect();
    assert_eq!(ids, expected);

    Ok(())
}

#[tokio::test]
async fn thread_list_updated_at_tie_breaks_by_uuid() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let id_a = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let id_b = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T11-00-00",
        "2025-02-01T11:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let updated_at = "2025-02-03T00:00:00Z";
    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-02-01T10-00-00", &id_a).as_path(),
        updated_at,
    )?;
    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-02-01T11-00-00", &id_b).as_path(),
        updated_at,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads_with_sort(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        Some(ThreadSortKey::UpdatedAt),
        /*archived*/ None,
    )
    .await?;

    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    let mut expected = [id_a, id_b];
    expected.sort_by_key(|id| Reverse(Uuid::parse_str(id).expect("uuid should parse")));
    let expected: Vec<_> = expected.iter().map(String::as_str).collect();
    assert_eq!(ids, expected);

    Ok(())
}

#[tokio::test]
async fn thread_list_updated_at_uses_mtime() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let thread_id = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "Hello",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    set_rollout_mtime(
        rollout_path(praxis_home.path(), "2025-02-01T10-00-00", &thread_id).as_path(),
        "2025-02-05T00:00:00Z",
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads_with_sort(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        Some(ThreadSortKey::UpdatedAt),
        /*archived*/ None,
    )
    .await?;

    let thread = data
        .iter()
        .find(|item| item.id == thread_id)
        .expect("expected thread for created rollout");
    let expected_created =
        chrono::DateTime::parse_from_rfc3339("2025-02-01T10:00:00Z")?.timestamp();
    let expected_updated =
        chrono::DateTime::parse_from_rfc3339("2025-02-05T00:00:00Z")?.timestamp();
    assert_eq!(thread.created_at, expected_created);
    assert_eq!(thread.updated_at, expected_updated);

    Ok(())
}

#[tokio::test]
async fn thread_list_archived_filter() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let active_id = create_fake_rollout(
        praxis_home.path(),
        "2025-03-01T10-00-00",
        "2025-03-01T10:00:00Z",
        "Active",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let archived_id = create_fake_rollout(
        praxis_home.path(),
        "2025-03-01T09-00-00",
        "2025-03-01T09:00:00Z",
        "Archived",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let archived_dir = praxis_home.path().join(ARCHIVED_SESSIONS_SUBDIR);
    fs::create_dir_all(&archived_dir)?;
    let archived_source = rollout_path(praxis_home.path(), "2025-03-01T09-00-00", &archived_id);
    let archived_dest = archived_dir.join(
        archived_source
            .file_name()
            .expect("archived rollout should have a file name"),
    );
    fs::rename(&archived_source, &archived_dest)?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse { data, .. } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].id, active_id);

    let ThreadListResponse { data, .. } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        Some(true),
    )
    .await?;
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].id, archived_id);

    Ok(())
}

#[tokio::test]
async fn thread_list_invalid_cursor_returns_error() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let request_id = mcp
        .send_thread_list_request(praxis_app_gateway_protocol::ThreadListParams {
            cursor: Some("not-a-cursor".to_string()),
            limit: Some(2),
            sort_key: None,
            model_providers: Some(vec!["mock_provider".to_string()]),
            source_kinds: None,
            archived: None,
            cwd: None,
            cwd_scope: None,
            search_term: None,
        })
        .await?;
    let error: JSONRPCError = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert_eq!(error.error.code, -32600);
    assert_eq!(error.error.message, "invalid cursor: not-a-cursor");

    Ok(())
}
