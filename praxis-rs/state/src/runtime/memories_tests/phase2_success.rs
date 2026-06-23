use super::*;

#[tokio::test]
async fn mark_global_phase2_job_succeeded_updates_selected_snapshot_timestamp() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let thread_id = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id");
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            thread_id,
            praxis_home.join("workspace"),
        ))
        .await
        .expect("upsert thread");

    let initial_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim initial stage1");
    let initial_token = match initial_claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected stage1 claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_id,
                initial_token.as_str(),
                /*source_updated_at*/ 100,
                "raw-100",
                "summary-100",
                Some("rollout-100"),
            )
            .await
            .expect("mark initial stage1 success"),
        "initial stage1 success should persist output"
    );

    let first_phase2_claim = runtime
        .try_claim_global_phase2_job(owner, /*lease_seconds*/ 3600)
        .await
        .expect("claim first phase2");
    let (first_phase2_token, first_input_watermark) = match first_phase2_claim {
        Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } => (ownership_token, input_watermark),
        other => panic!("unexpected first phase2 claim outcome: {other:?}"),
    };
    let first_selected_outputs = runtime
        .list_stage1_outputs_for_global(/*n*/ 1)
        .await
        .expect("list first selected outputs");
    assert!(
        runtime
            .mark_global_phase2_job_succeeded(
                first_phase2_token.as_str(),
                first_input_watermark,
                &first_selected_outputs,
            )
            .await
            .expect("mark first phase2 success"),
        "first phase2 success should persist selected rows"
    );

    let refreshed_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 101, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim refreshed stage1");
    let refreshed_token = match refreshed_claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected refreshed stage1 claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_id,
                refreshed_token.as_str(),
                /*source_updated_at*/ 101,
                "raw-101",
                "summary-101",
                Some("rollout-101"),
            )
            .await
            .expect("mark refreshed stage1 success"),
        "refreshed stage1 success should persist output"
    );

    let second_phase2_claim = runtime
        .try_claim_global_phase2_job(owner, /*lease_seconds*/ 3600)
        .await
        .expect("claim second phase2");
    let (second_phase2_token, second_input_watermark) = match second_phase2_claim {
        Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } => (ownership_token, input_watermark),
        other => panic!("unexpected second phase2 claim outcome: {other:?}"),
    };
    let second_selected_outputs = runtime
        .list_stage1_outputs_for_global(/*n*/ 1)
        .await
        .expect("list second selected outputs");
    assert_eq!(
        second_selected_outputs[0].source_updated_at.timestamp(),
        101
    );
    assert!(
        runtime
            .mark_global_phase2_job_succeeded(
                second_phase2_token.as_str(),
                second_input_watermark,
                &second_selected_outputs,
            )
            .await
            .expect("mark second phase2 success"),
        "second phase2 success should persist selected rows"
    );

    let selection = runtime
        .get_phase2_input_selection(/*n*/ 1, /*max_unused_days*/ 36_500)
        .await
        .expect("load phase2 input selection after refresh");
    assert_eq!(selection.retained_thread_ids, vec![thread_id]);

    let (selected_for_phase2, selected_for_phase2_source_updated_at) =
        sqlx::query_as::<_, (i64, Option<i64>)>(
            "SELECT selected_for_phase2, selected_for_phase2_source_updated_at FROM stage1_outputs WHERE thread_id = ?",
        )
        .bind(thread_id.to_string())
        .fetch_one(runtime.pool.as_ref())
        .await
        .expect("load selected snapshot after phase2");
    assert_eq!(selected_for_phase2, 1);
    assert_eq!(selected_for_phase2_source_updated_at, Some(101));

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn mark_global_phase2_job_succeeded_only_marks_exact_selected_snapshots() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let thread_id = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id");
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            thread_id,
            praxis_home.join("workspace"),
        ))
        .await
        .expect("upsert thread");

    let initial_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim initial stage1");
    let initial_token = match initial_claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected stage1 claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_id,
                initial_token.as_str(),
                /*source_updated_at*/ 100,
                "raw-100",
                "summary-100",
                Some("rollout-100"),
            )
            .await
            .expect("mark initial stage1 success"),
        "initial stage1 success should persist output"
    );

    let phase2_claim = runtime
        .try_claim_global_phase2_job(owner, /*lease_seconds*/ 3600)
        .await
        .expect("claim phase2");
    let (phase2_token, input_watermark) = match phase2_claim {
        Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } => (ownership_token, input_watermark),
        other => panic!("unexpected phase2 claim outcome: {other:?}"),
    };
    let selected_outputs = runtime
        .list_stage1_outputs_for_global(/*n*/ 1)
        .await
        .expect("list selected outputs");
    assert_eq!(selected_outputs[0].source_updated_at.timestamp(), 100);

    let refreshed_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 101, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim refreshed stage1");
    let refreshed_token = match refreshed_claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected stage1 claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_id,
                refreshed_token.as_str(),
                /*source_updated_at*/ 101,
                "raw-101",
                "summary-101",
                Some("rollout-101"),
            )
            .await
            .expect("mark refreshed stage1 success"),
        "refreshed stage1 success should persist output"
    );

    assert!(
        runtime
            .mark_global_phase2_job_succeeded(
                phase2_token.as_str(),
                input_watermark,
                &selected_outputs,
            )
            .await
            .expect("mark phase2 success"),
        "phase2 success should still complete"
    );

    let (selected_for_phase2, selected_for_phase2_source_updated_at) =
        sqlx::query_as::<_, (i64, Option<i64>)>(
            "SELECT selected_for_phase2, selected_for_phase2_source_updated_at FROM stage1_outputs WHERE thread_id = ?",
        )
        .bind(thread_id.to_string())
        .fetch_one(runtime.pool.as_ref())
        .await
        .expect("load selected_for_phase2");
    assert_eq!(selected_for_phase2, 0);
    assert_eq!(selected_for_phase2_source_updated_at, None);

    let selection = runtime
        .get_phase2_input_selection(/*n*/ 1, /*max_unused_days*/ 36_500)
        .await
        .expect("load phase2 input selection");
    assert_eq!(selection.selected.len(), 1);
    assert_eq!(selection.selected[0].source_updated_at.timestamp(), 101);
    assert!(selection.retained_thread_ids.is_empty());

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}
