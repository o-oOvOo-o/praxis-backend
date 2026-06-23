use super::*;

#[tokio::test]
async fn thread_list_fetches_until_limit_or_exhausted() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    // Newest 16 conversations belong to a different provider; the older 8 are the
    // only ones that match the filter. We request 8 so the server must keep
    // paging past the first two pages to reach the desired count.
    create_fake_rollouts(
        praxis_home.path(),
        /*count*/ 24,
        |i| {
            if i < 16 {
                "skip_provider"
            } else {
                "target_provider"
            }
        },
        |i| {
            timestamp_at(
                /*year*/ 2025,
                /*month*/ 3,
                30 - i as u32,
                /*hour*/ 12,
                /*minute*/ 0,
                /*second*/ 0,
            )
        },
        "Hello",
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    // Request 8 threads for the target provider; the matches only start on the
    // third page so we rely on pagination to reach the limit.
    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(8),
        Some(vec!["target_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert_eq!(
        data.len(),
        8,
        "should keep paging until the requested count is filled"
    );
    assert!(
        data.iter()
            .all(|thread| thread.model_provider == "target_provider"),
        "all returned threads must match the requested provider"
    );
    assert_eq!(
        next_cursor, None,
        "once the requested count is satisfied on the final page, nextCursor should be None"
    );

    Ok(())
}

#[tokio::test]
async fn thread_list_enforces_max_limit() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    create_fake_rollouts(
        praxis_home.path(),
        /*count*/ 105,
        |_| "mock_provider",
        |i| {
            let month = 5 + (i / 28);
            let day = (i % 28) + 1;
            timestamp_at(
                /*year*/ 2025,
                month as u32,
                day as u32,
                /*hour*/ 0,
                /*minute*/ 0,
                /*second*/ 0,
            )
        },
        "Hello",
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(200),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert_eq!(
        data.len(),
        100,
        "limit should be clamped to the maximum page size"
    );
    assert!(
        next_cursor.is_some(),
        "when more than the maximum exist, nextCursor should continue pagination"
    );

    Ok(())
}

#[tokio::test]
async fn thread_list_stops_when_not_enough_filtered_results_exist() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    // Only the last 7 conversations match the provider filter; we ask for 10 to
    // ensure the server exhausts pagination without looping forever.
    create_fake_rollouts(
        praxis_home.path(),
        /*count*/ 22,
        |i| {
            if i < 15 {
                "skip_provider"
            } else {
                "target_provider"
            }
        },
        |i| {
            timestamp_at(
                /*year*/ 2025,
                /*month*/ 4,
                28 - i as u32,
                /*hour*/ 8,
                /*minute*/ 0,
                /*second*/ 0,
            )
        },
        "Hello",
    )?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    // Request more threads than exist after filtering; expect all matches to be
    // returned with nextCursor None.
    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["target_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert_eq!(
        data.len(),
        7,
        "all available filtered threads should be returned"
    );
    assert!(
        data.iter()
            .all(|thread| thread.model_provider == "target_provider"),
        "results should still respect the provider filter"
    );
    assert_eq!(
        next_cursor, None,
        "when results are exhausted before reaching the limit, nextCursor should be None"
    );

    Ok(())
}
