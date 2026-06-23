use super::*;

#[tokio::test]
async fn record_stage1_output_usage_updates_usage_metadata() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let thread_a = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id a");
    let thread_b = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id b");
    let missing = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("missing id");
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");

    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            thread_a,
            praxis_home.join("workspace-a"),
        ))
        .await
        .expect("upsert thread a");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            thread_b,
            praxis_home.join("workspace-b"),
        ))
        .await
        .expect("upsert thread b");

    let claim_a = runtime
        .try_claim_stage1_job(
            thread_a, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1 a");
    let token_a = match claim_a {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected stage1 claim outcome for a: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_a,
                token_a.as_str(),
                /*source_updated_at*/ 100,
                "raw a",
                "sum a",
                /*rollout_slug*/ None
            )
            .await
            .expect("mark stage1 succeeded a")
    );

    let claim_b = runtime
        .try_claim_stage1_job(
            thread_b, owner, /*source_updated_at*/ 101, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1 b");
    let token_b = match claim_b {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected stage1 claim outcome for b: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_b,
                token_b.as_str(),
                /*source_updated_at*/ 101,
                "raw b",
                "sum b",
                /*rollout_slug*/ None
            )
            .await
            .expect("mark stage1 succeeded b")
    );

    let updated_rows = runtime
        .record_stage1_output_usage(&[thread_a, thread_a, thread_b, missing])
        .await
        .expect("record stage1 output usage");
    assert_eq!(updated_rows, 3);

    let row_a =
        sqlx::query("SELECT usage_count, last_usage FROM stage1_outputs WHERE thread_id = ?")
            .bind(thread_a.to_string())
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("load stage1 usage row a");
    let row_b =
        sqlx::query("SELECT usage_count, last_usage FROM stage1_outputs WHERE thread_id = ?")
            .bind(thread_b.to_string())
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("load stage1 usage row b");

    assert_eq!(
        row_a
            .try_get::<i64, _>("usage_count")
            .expect("usage_count a"),
        2
    );
    assert_eq!(
        row_b
            .try_get::<i64, _>("usage_count")
            .expect("usage_count b"),
        1
    );

    let last_usage_a = row_a.try_get::<i64, _>("last_usage").expect("last_usage a");
    let last_usage_b = row_b.try_get::<i64, _>("last_usage").expect("last_usage b");
    assert_eq!(last_usage_a, last_usage_b);
    assert!(last_usage_a > 0);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn get_phase2_input_selection_prioritizes_usage_count_then_recent_usage() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let now = Utc::now();
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let thread_a = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id a");
    let thread_b = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id b");
    let thread_c = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id c");

    for (thread_id, workspace) in [
        (thread_a, "workspace-a"),
        (thread_b, "workspace-b"),
        (thread_c, "workspace-c"),
    ] {
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                thread_id,
                praxis_home.join(workspace),
            ))
            .await
            .expect("upsert thread");
    }

    for (thread_id, generated_at, summary) in [
        (thread_a, now - Duration::days(3), "summary-a"),
        (thread_b, now - Duration::days(2), "summary-b"),
        (thread_c, now - Duration::days(1), "summary-c"),
    ] {
        let source_updated_at = generated_at.timestamp();
        let claim = runtime
            .try_claim_stage1_job(
                thread_id,
                owner,
                source_updated_at,
                /*lease_seconds*/ 3600,
                /*max_running_jobs*/ 64,
            )
            .await
            .expect("claim stage1");
        let ownership_token = match claim {
            Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
            other => panic!("unexpected stage1 claim outcome: {other:?}"),
        };
        assert!(
            runtime
                .mark_stage1_job_succeeded(
                    thread_id,
                    ownership_token.as_str(),
                    source_updated_at,
                    &format!("raw-{summary}"),
                    summary,
                    /*rollout_slug*/ None,
                )
                .await
                .expect("mark stage1 success"),
            "stage1 success should persist output"
        );
    }

    for (thread_id, usage_count, last_usage) in [
        (thread_a, 5_i64, now - Duration::days(10)),
        (thread_b, 5_i64, now - Duration::days(1)),
        (thread_c, 1_i64, now - Duration::hours(1)),
    ] {
        sqlx::query(
            "UPDATE stage1_outputs SET usage_count = ?, last_usage = ? WHERE thread_id = ?",
        )
        .bind(usage_count)
        .bind(last_usage.timestamp())
        .bind(thread_id.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("update usage metadata");
    }

    let selection = runtime
        .get_phase2_input_selection(/*n*/ 3, /*max_unused_days*/ 30)
        .await
        .expect("load phase2 input selection");

    assert_eq!(
        selection
            .selected
            .iter()
            .map(|output| output.thread_id)
            .collect::<Vec<_>>(),
        vec![thread_b, thread_a, thread_c]
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn get_phase2_input_selection_excludes_stale_used_memories_but_keeps_fresh_never_used() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let now = Utc::now();
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let thread_a = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id a");
    let thread_b = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id b");
    let thread_c = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id c");

    for (thread_id, workspace) in [
        (thread_a, "workspace-a"),
        (thread_b, "workspace-b"),
        (thread_c, "workspace-c"),
    ] {
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                thread_id,
                praxis_home.join(workspace),
            ))
            .await
            .expect("upsert thread");
    }

    for (thread_id, generated_at, summary) in [
        (thread_a, now - Duration::days(40), "summary-a"),
        (thread_b, now - Duration::days(2), "summary-b"),
        (thread_c, now - Duration::days(50), "summary-c"),
    ] {
        let source_updated_at = generated_at.timestamp();
        let claim = runtime
            .try_claim_stage1_job(
                thread_id,
                owner,
                source_updated_at,
                /*lease_seconds*/ 3600,
                /*max_running_jobs*/ 64,
            )
            .await
            .expect("claim stage1");
        let ownership_token = match claim {
            Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
            other => panic!("unexpected stage1 claim outcome: {other:?}"),
        };
        assert!(
            runtime
                .mark_stage1_job_succeeded(
                    thread_id,
                    ownership_token.as_str(),
                    source_updated_at,
                    &format!("raw-{summary}"),
                    summary,
                    /*rollout_slug*/ None,
                )
                .await
                .expect("mark stage1 success"),
            "stage1 success should persist output"
        );
    }

    for (thread_id, usage_count, last_usage) in [
        (thread_a, Some(9_i64), Some(now - Duration::days(31))),
        (thread_b, None, None),
        (thread_c, Some(1_i64), Some(now - Duration::days(1))),
    ] {
        sqlx::query(
            "UPDATE stage1_outputs SET usage_count = ?, last_usage = ? WHERE thread_id = ?",
        )
        .bind(usage_count)
        .bind(last_usage.map(|value| value.timestamp()))
        .bind(thread_id.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("update usage metadata");
    }

    let selection = runtime
        .get_phase2_input_selection(/*n*/ 3, /*max_unused_days*/ 30)
        .await
        .expect("load phase2 input selection");

    assert_eq!(
        selection
            .selected
            .iter()
            .map(|output| output.thread_id)
            .collect::<Vec<_>>(),
        vec![thread_c, thread_b]
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn get_phase2_input_selection_prefers_recent_thread_updates_over_recent_generation() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let older_thread = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("older thread id");
    let newer_thread = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("newer thread id");

    for (thread_id, workspace) in [
        (older_thread, "workspace-older"),
        (newer_thread, "workspace-newer"),
    ] {
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                thread_id,
                praxis_home.join(workspace),
            ))
            .await
            .expect("upsert thread");
    }

    for (thread_id, source_updated_at, summary) in [
        (older_thread, 100_i64, "summary-older"),
        (newer_thread, 200_i64, "summary-newer"),
    ] {
        let claim = runtime
            .try_claim_stage1_job(
                thread_id,
                owner,
                source_updated_at,
                /*lease_seconds*/ 3600,
                /*max_running_jobs*/ 64,
            )
            .await
            .expect("claim stage1");
        let ownership_token = match claim {
            Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
            other => panic!("unexpected stage1 claim outcome: {other:?}"),
        };
        assert!(
            runtime
                .mark_stage1_job_succeeded(
                    thread_id,
                    ownership_token.as_str(),
                    source_updated_at,
                    &format!("raw-{summary}"),
                    summary,
                    /*rollout_slug*/ None,
                )
                .await
                .expect("mark stage1 success"),
            "stage1 success should persist output"
        );
    }

    sqlx::query("UPDATE stage1_outputs SET generated_at = ? WHERE thread_id = ?")
        .bind(300_i64)
        .bind(older_thread.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("update older generated_at");
    sqlx::query("UPDATE stage1_outputs SET generated_at = ? WHERE thread_id = ?")
        .bind(150_i64)
        .bind(newer_thread.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("update newer generated_at");

    let selection = runtime
        .get_phase2_input_selection(/*n*/ 1, /*max_unused_days*/ 36_500)
        .await
        .expect("load phase2 input selection");

    assert_eq!(selection.selected.len(), 1);
    assert_eq!(selection.selected[0].thread_id, newer_thread);
    assert_eq!(selection.selected[0].source_updated_at.timestamp(), 200);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn prune_stage1_outputs_for_retention_prunes_stale_unselected_rows_only() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let stale_unused = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("stale unused");
    let stale_used = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("stale used");
    let stale_selected =
        ThreadId::from_string(&Uuid::new_v4().to_string()).expect("stale selected");
    let fresh_used = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("fresh used");

    for (thread_id, workspace) in [
        (stale_unused, "workspace-stale-unused"),
        (stale_used, "workspace-stale-used"),
        (stale_selected, "workspace-stale-selected"),
        (fresh_used, "workspace-fresh-used"),
    ] {
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                thread_id,
                praxis_home.join(workspace),
            ))
            .await
            .expect("upsert thread");
    }

    let now = Utc::now().timestamp();
    for (thread_id, source_updated_at, summary) in [
        (
            stale_unused,
            now - Duration::days(60).num_seconds(),
            "stale-unused",
        ),
        (
            stale_used,
            now - Duration::days(50).num_seconds(),
            "stale-used",
        ),
        (
            stale_selected,
            now - Duration::days(45).num_seconds(),
            "stale-selected",
        ),
        (
            fresh_used,
            now - Duration::days(10).num_seconds(),
            "fresh-used",
        ),
    ] {
        let claim = runtime
            .try_claim_stage1_job(
                thread_id,
                owner,
                source_updated_at,
                /*lease_seconds*/ 3600,
                /*max_running_jobs*/ 64,
            )
            .await
            .expect("claim stage1");
        let ownership_token = match claim {
            Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
            other => panic!("unexpected stage1 claim outcome: {other:?}"),
        };
        assert!(
            runtime
                .mark_stage1_job_succeeded(
                    thread_id,
                    ownership_token.as_str(),
                    source_updated_at,
                    &format!("raw-{summary}"),
                    summary,
                    /*rollout_slug*/ None,
                )
                .await
                .expect("mark stage1 success"),
            "stage1 success should persist output"
        );
    }

    sqlx::query("UPDATE stage1_outputs SET usage_count = ?, last_usage = ? WHERE thread_id = ?")
        .bind(3_i64)
        .bind(now - Duration::days(40).num_seconds())
        .bind(stale_used.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("set stale used metadata");
    sqlx::query(
        "UPDATE stage1_outputs SET selected_for_phase2 = 1, selected_for_phase2_source_updated_at = source_updated_at WHERE thread_id = ?",
    )
    .bind(stale_selected.to_string())
    .execute(runtime.pool.as_ref())
    .await
    .expect("mark selected for phase2");
    sqlx::query("UPDATE stage1_outputs SET usage_count = ?, last_usage = ? WHERE thread_id = ?")
        .bind(8_i64)
        .bind(now - Duration::days(2).num_seconds())
        .bind(fresh_used.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("set fresh used metadata");

    let before_jobs_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE kind = 'memory_stage1'")
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("count stage1 jobs before prune");

    let pruned = runtime
        .prune_stage1_outputs_for_retention(/*max_unused_days*/ 30, /*limit*/ 100)
        .await
        .expect("prune stage1 outputs");
    assert_eq!(pruned, 2);

    let remaining =
        sqlx::query_scalar::<_, String>("SELECT thread_id FROM stage1_outputs ORDER BY thread_id")
            .fetch_all(runtime.pool.as_ref())
            .await
            .expect("load remaining stage1 outputs");
    let mut expected_remaining = vec![fresh_used.to_string(), stale_selected.to_string()];
    expected_remaining.sort();
    assert_eq!(remaining, expected_remaining);

    let after_jobs_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE kind = 'memory_stage1'")
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("count stage1 jobs after prune");
    assert_eq!(after_jobs_count, before_jobs_count);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn prune_stage1_outputs_for_retention_respects_batch_limit() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let thread_a = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread a");
    let thread_b = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread b");
    let thread_c = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread c");

    for (thread_id, workspace) in [
        (thread_a, "workspace-a"),
        (thread_b, "workspace-b"),
        (thread_c, "workspace-c"),
    ] {
        runtime
            .upsert_thread(&test_thread_metadata(
                &praxis_home,
                thread_id,
                praxis_home.join(workspace),
            ))
            .await
            .expect("upsert thread");
    }

    let now = Utc::now().timestamp();
    for (thread_id, source_updated_at, summary) in [
        (thread_a, now - Duration::days(60).num_seconds(), "stale-a"),
        (thread_b, now - Duration::days(50).num_seconds(), "stale-b"),
        (thread_c, now - Duration::days(40).num_seconds(), "stale-c"),
    ] {
        let claim = runtime
            .try_claim_stage1_job(
                thread_id,
                owner,
                source_updated_at,
                /*lease_seconds*/ 3600,
                /*max_running_jobs*/ 64,
            )
            .await
            .expect("claim stage1");
        let ownership_token = match claim {
            Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
            other => panic!("unexpected stage1 claim outcome: {other:?}"),
        };
        assert!(
            runtime
                .mark_stage1_job_succeeded(
                    thread_id,
                    ownership_token.as_str(),
                    source_updated_at,
                    &format!("raw-{summary}"),
                    summary,
                    /*rollout_slug*/ None,
                )
                .await
                .expect("mark stage1 success"),
            "stage1 success should persist output"
        );
    }

    let pruned = runtime
        .prune_stage1_outputs_for_retention(/*max_unused_days*/ 30, /*limit*/ 2)
        .await
        .expect("prune stage1 outputs with limit");
    assert_eq!(pruned, 2);

    let remaining_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stage1_outputs")
        .fetch_one(runtime.pool.as_ref())
        .await
        .expect("count remaining stage1 outputs");
    assert_eq!(remaining_count, 1);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}
