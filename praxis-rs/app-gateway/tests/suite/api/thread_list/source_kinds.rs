use super::*;

#[tokio::test]
async fn thread_list_empty_source_kinds_defaults_to_interactive_only() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let cli_id = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "CLI",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let exec_id = create_fake_rollout_with_source(
        praxis_home.path(),
        "2025-02-01T11-00-00",
        "2025-02-01T11:00:00Z",
        "Exec",
        Some("mock_provider"),
        /*git_info*/ None,
        CoreSessionSource::Exec,
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(Vec::new()),
        /*archived*/ None,
    )
    .await?;

    assert_eq!(next_cursor, None);
    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids, vec![cli_id.as_str()]);
    assert_ne!(cli_id, exec_id);
    assert_eq!(data[0].source, SessionSource::Cli);

    Ok(())
}

#[tokio::test]
async fn thread_list_filters_by_source_kind_subagent_thread_spawn() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let cli_id = create_fake_rollout(
        praxis_home.path(),
        "2025-02-01T10-00-00",
        "2025-02-01T10:00:00Z",
        "CLI",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let parent_thread_id = ThreadId::from_string(&Uuid::new_v4().to_string())?;
    let subagent_id = create_fake_rollout_with_source(
        praxis_home.path(),
        "2025-02-01T11-00-00",
        "2025-02-01T11:00:00Z",
        "SubAgent",
        Some("mock_provider"),
        /*git_info*/ None,
        CoreSessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth: 1,
            agent_path: None,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
        }),
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(vec![ThreadSourceKind::SubAgentThreadSpawn]),
        /*archived*/ None,
    )
    .await?;

    assert_eq!(next_cursor, None);
    let ids: Vec<_> = data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(ids, vec![subagent_id.as_str()]);
    assert_ne!(cli_id, subagent_id);
    assert!(matches!(data[0].source, SessionSource::SubAgent(_)));

    Ok(())
}

#[tokio::test]
async fn thread_list_filters_by_subagent_variant() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let parent_thread_id = ThreadId::from_string(&Uuid::new_v4().to_string())?;

    let review_id = create_fake_rollout_with_source(
        praxis_home.path(),
        "2025-02-02T09-00-00",
        "2025-02-02T09:00:00Z",
        "Review",
        Some("mock_provider"),
        /*git_info*/ None,
        CoreSessionSource::SubAgent(SubAgentSource::Review),
    )?;
    let compact_id = create_fake_rollout_with_source(
        praxis_home.path(),
        "2025-02-02T10-00-00",
        "2025-02-02T10:00:00Z",
        "Compact",
        Some("mock_provider"),
        /*git_info*/ None,
        CoreSessionSource::SubAgent(SubAgentSource::Compact),
    )?;
    let spawn_id = create_fake_rollout_with_source(
        praxis_home.path(),
        "2025-02-02T11-00-00",
        "2025-02-02T11:00:00Z",
        "Spawn",
        Some("mock_provider"),
        /*git_info*/ None,
        CoreSessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth: 1,
            agent_path: None,
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
        }),
    )?;
    let other_id = create_fake_rollout_with_source(
        praxis_home.path(),
        "2025-02-02T12-00-00",
        "2025-02-02T12:00:00Z",
        "Other",
        Some("mock_provider"),
        /*git_info*/ None,
        CoreSessionSource::SubAgent(SubAgentSource::Other("custom".to_string())),
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let review = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(vec![ThreadSourceKind::SubAgentReview]),
        /*archived*/ None,
    )
    .await?;
    let review_ids: Vec<_> = review
        .data
        .iter()
        .map(|thread| thread.id.as_str())
        .collect();
    assert_eq!(review_ids, vec![review_id.as_str()]);

    let compact = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(vec![ThreadSourceKind::SubAgentCompact]),
        /*archived*/ None,
    )
    .await?;
    let compact_ids: Vec<_> = compact
        .data
        .iter()
        .map(|thread| thread.id.as_str())
        .collect();
    assert_eq!(compact_ids, vec![compact_id.as_str()]);

    let spawn = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(vec![ThreadSourceKind::SubAgentThreadSpawn]),
        /*archived*/ None,
    )
    .await?;
    let spawn_ids: Vec<_> = spawn.data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(spawn_ids, vec![spawn_id.as_str()]);

    let other = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(vec![ThreadSourceKind::SubAgentOther]),
        /*archived*/ None,
    )
    .await?;
    let other_ids: Vec<_> = other.data.iter().map(|thread| thread.id.as_str()).collect();
    assert_eq!(other_ids, vec![other_id.as_str()]);

    Ok(())
}
