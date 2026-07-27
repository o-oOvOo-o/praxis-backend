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
        let permissions_will_change = before.approval_policy != core_approval_policy
            || before.approvals_reviewer != core_approvals_reviewer
            || before.sandbox_policy != core_sandbox_policy;
        if permissions_will_change {
            self.outgoing
                .resolve_pending_approval_requests(
                    thread_uuid,
                    permission_changed_request_error(
                        thread.permission_generation().saturating_add(1),
                    ),
                )
                .await;
        }
        let generation = match thread
            .set_permissions(
                core_approval_policy,
                core_approvals_reviewer,
                core_sandbox_policy,
            )
            .await
        {
            Ok(generation) => generation,
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to apply thread permissions for {thread_uuid}: {err}"),
                )
                .await;
                return;
            }
        };
        let after = thread.config_snapshot().await;

        if after.approval_policy == praxis_protocol::protocol::AskForApproval::Never
            && matches!(
                after.sandbox_policy,
                praxis_protocol::protocol::SandboxPolicy::DangerFullAccess
            )
        {
            self.outgoing
                .resolve_pending_approval_requests(
                    thread_uuid,
                    permission_changed_request_error(generation),
                )
                .await;
        }

        let mut thread_projection =
            build_thread_from_snapshot(thread_uuid, &after, thread.rollout_path());
        self.attach_thread_name(thread_uuid, &mut thread_projection)
            .await;
        self.project_thread_runtime_state(&mut thread_projection, false)
            .await;

        let response = ThreadPermissionsSetResponse {
            thread: thread_projection.clone(),
            generation,
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
                        generation,
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
}

fn permission_changed_request_error(
    generation: u64,
) -> praxis_app_gateway_protocol::JSONRPCErrorError {
    praxis_app_gateway_protocol::JSONRPCErrorError {
        code: crate::error_code::INTERNAL_ERROR_CODE,
        message: "approval resolved because thread permissions changed".to_string(),
        data: Some(serde_json::json!({
            "reason": crate::server_request_error::PERMISSION_CHANGED_PENDING_REQUEST_ERROR_REASON,
            "generation": generation,
        })),
    }
}
