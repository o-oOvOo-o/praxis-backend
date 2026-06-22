use super::thread_projection_api::build_thread_from_snapshot;
use super::*;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_model_set(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadModelSetParams,
    ) {
        let ThreadModelSetParams {
            thread_id,
            model_provider,
            model,
            reasoning_effort,
        } = params;

        let model_provider = model_provider.trim().to_owned();
        if model_provider.is_empty() {
            self.send_invalid_request_error(
                request_id,
                "modelProvider must not be empty".to_owned(),
            )
            .await;
            return;
        }

        let model = model.trim().to_owned();
        if model.is_empty() {
            self.send_invalid_request_error(request_id, "model must not be empty".to_owned())
                .await;
            return;
        }

        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        let thread = match self.thread_manager.get_thread(thread_uuid).await {
            Ok(thread) => thread,
            Err(_) => {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "thread/model/set requires loaded thread {thread_uuid}; call thread/resume first"
                    ),
                )
                .await;
                return;
            }
        };

        let before = thread.config_snapshot().await;
        let op = Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: Some(model_provider.clone()),
            model: Some(model.clone()),
            effort: reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        };

        if let Err(err) = self.submit_core_op(&request_id, thread.as_ref(), op).await {
            self.send_internal_error(
                request_id,
                format!("failed to apply thread model override for {thread_uuid}: {err}"),
            )
            .await;
            return;
        }

        let Some(after) = self
            .wait_for_thread_model_snapshot(
                request_id.clone(),
                thread_uuid,
                thread.as_ref(),
                &model_provider,
                &model,
                reasoning_effort,
            )
            .await
        else {
            return;
        };

        let mut thread_projection =
            build_thread_from_snapshot(thread_uuid, &after, thread.rollout_path());
        self.attach_thread_name(thread_uuid, &mut thread_projection)
            .await;
        self.project_thread_runtime_state(&mut thread_projection, false)
            .await;

        let response = ThreadModelSetResponse {
            thread: thread_projection.clone(),
            previous_model_provider: before.model_provider_id.clone(),
            previous_model: before.model.clone(),
            previous_reasoning_effort: before.reasoning_effort,
            model_provider: after.model_provider_id.clone(),
            model: after.model.clone(),
            reasoning_effort: after.reasoning_effort,
        };
        self.outgoing.send_response(request_id, response).await;

        if before.model_provider_id != after.model_provider_id
            || before.model != after.model
            || before.reasoning_effort != after.reasoning_effort
        {
            self.outgoing
                .send_server_notification(ServerNotification::ThreadModelChanged(
                    ThreadModelChangedNotification {
                        thread_id: thread_uuid.to_string(),
                        thread: thread_projection,
                        previous_model_provider: before.model_provider_id,
                        previous_model: before.model,
                        previous_reasoning_effort: before.reasoning_effort,
                        model_provider: after.model_provider_id,
                        model: after.model,
                        reasoning_effort: after.reasoning_effort,
                    },
                ))
                .await;
        }
    }

    async fn wait_for_thread_model_snapshot(
        &self,
        request_id: ConnectionRequestId,
        thread_id: ThreadId,
        thread: &PraxisThread,
        model_provider: &str,
        model: &str,
        reasoning_effort: Option<Option<praxis_protocol::openai_models::ReasoningEffort>>,
    ) -> Option<ThreadConfigSnapshot> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = thread.config_snapshot().await;
            let mismatches =
                thread_model_set_mismatches(&snapshot, model_provider, model, reasoning_effort);
            if mismatches.is_empty() {
                return Some(snapshot);
            }

            if tokio::time::Instant::now() >= deadline {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "thread/model/set did not become effective for running thread {thread_id}: {}",
                        mismatches.join("; ")
                    ),
                )
                .await;
                return None;
            }

            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

fn thread_model_set_mismatches(
    snapshot: &ThreadConfigSnapshot,
    model_provider: &str,
    model: &str,
    reasoning_effort: Option<Option<praxis_protocol::openai_models::ReasoningEffort>>,
) -> Vec<String> {
    let mut mismatches = Vec::new();
    if snapshot.model_provider_id != model_provider {
        mismatches.push(format!(
            "modelProvider expected `{model_provider}`, got `{}`",
            snapshot.model_provider_id
        ));
    }
    if snapshot.model != model {
        mismatches.push(format!(
            "model expected `{model}`, got `{}`",
            snapshot.model
        ));
    }
    if let Some(reasoning_effort) = reasoning_effort
        && snapshot.reasoning_effort != reasoning_effort
    {
        mismatches.push(format!(
            "reasoningEffort expected `{}`, got `{}`",
            display_reasoning_effort(reasoning_effort),
            display_reasoning_effort(snapshot.reasoning_effort)
        ));
    }
    mismatches
}

fn display_reasoning_effort(
    reasoning_effort: Option<praxis_protocol::openai_models::ReasoningEffort>,
) -> String {
    reasoning_effort
        .map(|effort| effort.to_string())
        .unwrap_or_else(|| "none".to_owned())
}
