use super::thread_projection_api::build_thread_from_snapshot;
use super::*;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_permissions_set(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadPermissionsSetParams,
    ) {
        let ThreadPermissionsSetParams {
            thread_id,
            approval_policy,
            approvals_reviewer,
            sandbox_policy,
        } = params;
        let core_approval_policy = approval_policy.to_core();
        let core_approvals_reviewer = approvals_reviewer.to_core();
        let core_sandbox_policy = sandbox_policy.to_core();

        if let Err(err) = self
            .config
            .permissions
            .approval_policy
            .can_set(&core_approval_policy)
        {
            self.send_invalid_request_error(request_id, format!("invalid approval policy: {err}"))
                .await;
            return;
        }
        if let Err(err) = self
            .config
            .permissions
            .sandbox_policy
            .can_set(&core_sandbox_policy)
        {
            self.send_invalid_request_error(request_id, format!("invalid sandbox policy: {err}"))
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
                        "thread/permissions/set requires loaded thread {thread_uuid}; call thread/resume first"
                    ),
                )
                .await;
                return;
            }
        };

        let before = thread.config_snapshot().await;
        let op = Op::OverrideTurnContext {
            cwd: None,
            approval_policy: Some(core_approval_policy),
            approvals_reviewer: Some(core_approvals_reviewer.clone()),
            sandbox_policy: Some(core_sandbox_policy.clone()),
            windows_sandbox_level: None,
            model_provider: None,
            model: None,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        };
        if let Err(err) = self.submit_core_op(&request_id, thread.as_ref(), op).await {
            self.send_internal_error(
                request_id,
                format!("failed to apply thread permission override for {thread_uuid}: {err}"),
            )
            .await;
            return;
        }

        let Some(after) = self
            .wait_for_thread_permissions_snapshot(
                request_id.clone(),
                thread_uuid,
                thread.as_ref(),
                core_approval_policy,
                core_approvals_reviewer,
                &core_sandbox_policy,
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

        let response = ThreadPermissionsSetResponse {
            thread: thread_projection.clone(),
            previous_approval_policy: before.approval_policy.into(),
            previous_approvals_reviewer: before.approvals_reviewer.into(),
            previous_sandbox_policy: before.sandbox_policy.clone().into(),
            approval_policy: after.approval_policy.into(),
            approvals_reviewer: after.approvals_reviewer.into(),
            sandbox_policy: after.sandbox_policy.clone().into(),
        };
        self.outgoing
            .send_response(request_id, response.clone())
            .await;

        if before.approval_policy != after.approval_policy
            || before.approvals_reviewer != after.approvals_reviewer
            || before.sandbox_policy != after.sandbox_policy
        {
            self.outgoing
                .send_server_notification(ServerNotification::ThreadPermissionsChanged(
                    ThreadPermissionsChangedNotification {
                        thread_id: thread_uuid.to_string(),
                        thread: thread_projection,
                        previous_approval_policy: response.previous_approval_policy,
                        previous_approvals_reviewer: response.previous_approvals_reviewer,
                        previous_sandbox_policy: response.previous_sandbox_policy,
                        approval_policy: response.approval_policy,
                        approvals_reviewer: response.approvals_reviewer,
                        sandbox_policy: response.sandbox_policy,
                    },
                ))
                .await;
        }
    }

    async fn wait_for_thread_permissions_snapshot(
        &self,
        request_id: ConnectionRequestId,
        thread_id: ThreadId,
        thread: &PraxisThread,
        approval_policy: praxis_protocol::protocol::AskForApproval,
        approvals_reviewer: praxis_protocol::config_types::ApprovalsReviewer,
        sandbox_policy: &praxis_protocol::protocol::SandboxPolicy,
    ) -> Option<ThreadConfigSnapshot> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = thread.config_snapshot().await;
            if snapshot.approval_policy == approval_policy
                && snapshot.approvals_reviewer == approvals_reviewer
                && snapshot.sandbox_policy == *sandbox_policy
            {
                return Some(snapshot);
            }
            if tokio::time::Instant::now() >= deadline {
                self.send_invalid_request_error(
                    request_id,
                    format!(
                        "thread/permissions/set did not become effective for running thread {thread_id}"
                    ),
                )
                .await;
                return None;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
