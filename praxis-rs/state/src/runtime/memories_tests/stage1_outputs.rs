use super::*;

#[tokio::test]
async fn stage1_output_cascades_on_thread_delete() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let thread_id = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id");
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let cwd = praxis_home.join("workspace");
    runtime
        .upsert_thread(&test_thread_metadata(&praxis_home, thread_id, cwd))
        .await
        .expect("upsert thread");

    let claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1");
    let ownership_token = match claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_id,
                ownership_token.as_str(),
                /*source_updated_at*/ 100,
                "raw",
                "sum",
                /*rollout_slug*/ None,
            )
            .await
            .expect("mark stage1 succeeded"),
        "mark stage1 succeeded should write stage1_outputs"
    );

    let count_before =
        sqlx::query("SELECT COUNT(*) AS count FROM stage1_outputs WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("count before delete")
            .try_get::<i64, _>("count")
            .expect("count value");
    assert_eq!(count_before, 1);

    sqlx::query("DELETE FROM threads WHERE id = ?")
        .bind(thread_id.to_string())
        .execute(runtime.pool.as_ref())
        .await
        .expect("delete thread");

    let count_after =
        sqlx::query("SELECT COUNT(*) AS count FROM stage1_outputs WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("count after delete")
            .try_get::<i64, _>("count")
            .expect("count value");
    assert_eq!(count_after, 0);

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn mark_stage1_job_succeeded_no_output_skips_phase2_when_output_was_already_absent() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let thread_id = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id");
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let owner_b = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            thread_id,
            praxis_home.join("workspace"),
        ))
        .await
        .expect("upsert thread");

    let claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1");
    let ownership_token = match claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded_no_output(thread_id, ownership_token.as_str())
            .await
            .expect("mark stage1 succeeded without output"),
        "stage1 no-output success should complete the job"
    );

    let output_row_count =
        sqlx::query("SELECT COUNT(*) AS count FROM stage1_outputs WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("load stage1 output count")
            .try_get::<i64, _>("count")
            .expect("stage1 output count");
    assert_eq!(
        output_row_count, 0,
        "stage1 no-output success should not persist empty stage1 outputs"
    );

    let up_to_date = runtime
        .try_claim_stage1_job(
            thread_id, owner_b, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1 up-to-date");
    assert_eq!(up_to_date, Stage1JobClaimOutcome::SkippedUpToDate);

    let global_job_row_count = sqlx::query("SELECT COUNT(*) AS count FROM jobs WHERE kind = ?")
        .bind("memory_consolidate_global")
        .fetch_one(runtime.pool.as_ref())
        .await
        .expect("load phase2 job row count")
        .try_get::<i64, _>("count")
        .expect("phase2 job row count");
    assert_eq!(
        global_job_row_count, 0,
        "no-output without an existing stage1 output should not enqueue phase2"
    );

    let claim_phase2 = runtime
        .try_claim_global_phase2_job(owner, /*lease_seconds*/ 3600)
        .await
        .expect("claim phase2");
    assert_eq!(
        claim_phase2,
        Phase2JobClaimOutcome::SkippedNotDirty,
        "phase2 should remain clean when no-output deleted nothing"
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn mark_stage1_job_succeeded_no_output_enqueues_phase2_when_deleting_output() {
    let praxis_home = unique_temp_dir();
    let runtime = StateRuntime::init(praxis_home.clone(), "test-provider".to_string())
        .await
        .expect("initialize runtime");

    let thread_id = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("thread id");
    let owner = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    let owner_b = ThreadId::from_string(&Uuid::new_v4().to_string()).expect("owner id");
    runtime
        .upsert_thread(&test_thread_metadata(
            &praxis_home,
            thread_id,
            praxis_home.join("workspace"),
        ))
        .await
        .expect("upsert thread");

    let first_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim initial stage1");
    let first_token = match first_claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected initial stage1 claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded(
                thread_id,
                first_token.as_str(),
                /*source_updated_at*/ 100,
                "raw",
                "sum",
                /*rollout_slug*/ None
            )
            .await
            .expect("mark initial stage1 succeeded"),
        "initial stage1 success should create stage1 output"
    );

    let phase2_claim = runtime
        .try_claim_global_phase2_job(owner, /*lease_seconds*/ 3600)
        .await
        .expect("claim phase2 after initial output");
    let (phase2_token, phase2_input_watermark) = match phase2_claim {
        Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } => (ownership_token, input_watermark),
        other => panic!("unexpected phase2 claim after initial output: {other:?}"),
    };
    assert_eq!(phase2_input_watermark, 100);
    assert!(
        runtime
            .mark_global_phase2_job_succeeded(phase2_token.as_str(), phase2_input_watermark, &[],)
            .await
            .expect("mark initial phase2 succeeded"),
        "initial phase2 success should clear global dirty state"
    );

    let no_output_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner_b, /*source_updated_at*/ 101, /*lease_seconds*/ 3600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1 for no-output delete");
    let no_output_token = match no_output_claim {
        Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
        other => panic!("unexpected no-output stage1 claim outcome: {other:?}"),
    };
    assert!(
        runtime
            .mark_stage1_job_succeeded_no_output(thread_id, no_output_token.as_str())
            .await
            .expect("mark stage1 no-output after existing output"),
        "no-output should succeed when deleting an existing stage1 output"
    );

    let output_row_count =
        sqlx::query("SELECT COUNT(*) AS count FROM stage1_outputs WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .fetch_one(runtime.pool.as_ref())
            .await
            .expect("load stage1 output count after delete")
            .try_get::<i64, _>("count")
            .expect("stage1 output count");
    assert_eq!(output_row_count, 0);

    let claim_phase2 = runtime
        .try_claim_global_phase2_job(owner, /*lease_seconds*/ 3600)
        .await
        .expect("claim phase2 after no-output deletion");
    let (phase2_token, phase2_input_watermark) = match claim_phase2 {
        Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } => (ownership_token, input_watermark),
        other => panic!("unexpected phase2 claim after no-output deletion: {other:?}"),
    };
    assert_eq!(phase2_input_watermark, 101);
    assert!(
        runtime
            .mark_global_phase2_job_succeeded(phase2_token.as_str(), phase2_input_watermark, &[],)
            .await
            .expect("mark phase2 succeeded after no-output delete")
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}

#[tokio::test]
async fn stage1_retry_exhaustion_does_not_block_newer_watermark() {
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

    for attempt in 0..3 {
        let claim = runtime
            .try_claim_stage1_job(
                thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3_600,
                /*max_running_jobs*/ 64,
            )
            .await
            .expect("claim stage1 for retry exhaustion");
        let ownership_token = match claim {
            Stage1JobClaimOutcome::Claimed { ownership_token } => ownership_token,
            other => panic!(
                "attempt {} should claim stage1 before retries are exhausted: {other:?}",
                attempt + 1
            ),
        };
        assert!(
            runtime
                .mark_stage1_job_failed(
                    thread_id,
                    ownership_token.as_str(),
                    "boom",
                    /*retry_delay_seconds*/ 0
                )
                .await
                .expect("mark stage1 failed"),
            "attempt {} should decrement retry budget",
            attempt + 1
        );
    }

    let exhausted_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 100, /*lease_seconds*/ 3_600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1 after retry exhaustion");
    assert_eq!(
        exhausted_claim,
        Stage1JobClaimOutcome::SkippedRetryExhausted
    );

    let newer_source_claim = runtime
        .try_claim_stage1_job(
            thread_id, owner, /*source_updated_at*/ 101, /*lease_seconds*/ 3_600,
            /*max_running_jobs*/ 64,
        )
        .await
        .expect("claim stage1 with newer source watermark");
    assert!(
        matches!(newer_source_claim, Stage1JobClaimOutcome::Claimed { .. }),
        "newer source watermark should reset retry budget and be claimable"
    );

    let job_row = sqlx::query(
        "SELECT retry_remaining, input_watermark FROM jobs WHERE kind = ? AND job_key = ?",
    )
    .bind("memory_stage1")
    .bind(thread_id.to_string())
    .fetch_one(runtime.pool.as_ref())
    .await
    .expect("load stage1 job row after newer-source claim");
    assert_eq!(
        job_row
            .try_get::<i64, _>("retry_remaining")
            .expect("retry_remaining"),
        3
    );
    assert_eq!(
        job_row
            .try_get::<i64, _>("input_watermark")
            .expect("input_watermark"),
        101
    );

    let _ = tokio::fs::remove_dir_all(praxis_home).await;
}
