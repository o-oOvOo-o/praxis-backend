use super::*;
use crate::thread_state::ThreadSelfworkState;
use praxis_app_core::selfwork::SELFWORK_STALL_LIMIT;
use praxis_app_core::selfwork::SelfworkPlanAdvance;
use praxis_app_core::selfwork::inspect_selfwork_plan;
use praxis_app_core::selfwork::selfwork_prompt;
use praxis_app_gateway_protocol::ThreadSelfworkGetParams;
use praxis_app_gateway_protocol::ThreadSelfworkGetResponse;
use praxis_app_gateway_protocol::ThreadSelfworkPhase;
use praxis_app_gateway_protocol::ThreadSelfworkStartParams;
use praxis_app_gateway_protocol::ThreadSelfworkStartResponse;
use praxis_app_gateway_protocol::ThreadSelfworkStatus;
use praxis_app_gateway_protocol::ThreadSelfworkStopParams;
use praxis_app_gateway_protocol::ThreadSelfworkStopResponse;
use praxis_app_gateway_protocol::ThreadSelfworkUpdatedNotification;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_protocol::user_input::UserInput as CoreUserInput;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_selfwork_get(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadSelfworkGetParams,
    ) {
        let Some((thread_id, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        self.restore_selfwork_for_thread(thread_id, &thread).await;
        let thread_state = self.thread_state_manager.thread_state(thread_id).await;
        let status = {
            let state = thread_state.lock().await;
            selfwork_status(thread_id, &state)
        };
        self.outgoing
            .send_response(request_id, ThreadSelfworkGetResponse { status })
            .await;
    }

    pub(crate) async fn thread_selfwork_start(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadSelfworkStartParams,
    ) {
        let Some((thread_id, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        let snapshot = thread.config_snapshot().await;
        let plan_path = if params.plan_path.is_absolute() {
            params.plan_path
        } else {
            snapshot.cwd.join(params.plan_path)
        };
        let inspection = match inspect_selfwork_plan(&plan_path) {
            Ok(inspection) if !inspection.complete => inspection,
            Ok(_) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("selfwork plan is already complete: {}", plan_path.display()),
                )
                .await;
                return;
            }
            Err(error) => {
                self.send_invalid_request_error(request_id, error).await;
                return;
            }
        };
        if let Err(error) = self
            .persist_selfwork_plan_path(thread_id, &thread, Some(inspection.path.as_path()))
            .await
        {
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let thread_state = self.thread_state_manager.thread_state(thread_id).await;
        {
            let mut state = thread_state.lock().await;
            let mut runtime = praxis_app_core::selfwork::SelfworkRuntimeState::default();
            runtime.arm(&inspection);
            state.selfwork = Some(ThreadSelfworkState {
                plan_path: inspection.path,
                runtime,
                active_turn_id: None,
            });
        }
        if !matches!(thread.agent_status().await, AgentStatus::Running)
            && let Err(error) = start_selfwork_turn(&thread_state, &thread).await
        {
            thread_state.lock().await.selfwork = None;
            let rollback_error = self
                .persist_selfwork_plan_path(thread_id, &thread, None)
                .await
                .err()
                .map(|rollback| format!("; rollback failed: {}", rollback.message))
                .unwrap_or_default();
            self.send_internal_error(request_id, format!("{error}{rollback_error}"))
                .await;
            return;
        }

        let status = {
            let state = thread_state.lock().await;
            selfwork_status(thread_id, &state)
        };
        self.outgoing
            .send_response(
                request_id,
                ThreadSelfworkStartResponse {
                    status: status.clone(),
                },
            )
            .await;
        self.broadcast_selfwork_updated(status).await;
    }

    pub(crate) async fn thread_selfwork_stop(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadSelfworkStopParams,
    ) {
        let Some((thread_id, thread)) = self
            .ensure_thread_for_request(&params.thread_id, &request_id)
            .await
        else {
            return;
        };
        if let Err(error) = self
            .persist_selfwork_plan_path(thread_id, &thread, None)
            .await
        {
            self.outgoing.send_error(request_id, error).await;
            return;
        }
        let thread_state = self.thread_state_manager.thread_state(thread_id).await;
        thread_state.lock().await.selfwork = None;
        let status = {
            let state = thread_state.lock().await;
            selfwork_status(thread_id, &state)
        };
        self.outgoing
            .send_response(
                request_id,
                ThreadSelfworkStopResponse {
                    status: status.clone(),
                },
            )
            .await;
        self.broadcast_selfwork_updated(status).await;
    }

    async fn persist_selfwork_plan_path(
        &self,
        thread_id: ThreadId,
        thread: &Arc<PraxisThread>,
        plan_path: Option<&Path>,
    ) -> Result<(), JSONRPCErrorError> {
        let state_db = thread
            .state_db()
            .or(get_state_db(&self.config).await)
            .ok_or_else(|| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("sqlite state db unavailable for thread {thread_id}"),
                data: None,
            })?;
        self.ensure_thread_metadata_row_exists(thread_id, &state_db, Some(thread))
            .await?;
        match state_db
            .update_thread_selfwork_plan_path(thread_id, plan_path)
            .await
        {
            Ok(true) => Ok(()),
            Ok(false) => Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("thread metadata disappeared before selfwork update: {thread_id}"),
                data: None,
            }),
            Err(error) => Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to persist selfwork for {thread_id}: {error}"),
                data: None,
            }),
        }
    }

    async fn broadcast_selfwork_updated(&self, status: ThreadSelfworkStatus) {
        self.outgoing
            .send_server_notification(ServerNotification::ThreadSelfworkUpdated(
                ThreadSelfworkUpdatedNotification { status },
            ))
            .await;
    }

    pub(crate) async fn restore_selfwork_for_thread(
        &self,
        thread_id: ThreadId,
        thread: &Arc<PraxisThread>,
    ) {
        let thread_state = self.thread_state_manager.thread_state(thread_id).await;
        if thread_state.lock().await.selfwork.is_some() {
            return;
        }
        let Some(state_db) = thread.state_db().or(get_state_db(&self.config).await) else {
            return;
        };
        let plan_path = match state_db.get_thread(thread_id).await {
            Ok(Some(metadata)) => metadata.selfwork_plan_path,
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(thread_id = %thread_id, %error, "failed to restore selfwork metadata");
                return;
            }
        };
        let Some(plan_path) = plan_path else {
            return;
        };
        let inspection = match inspect_selfwork_plan(&plan_path) {
            Ok(inspection) if !inspection.complete => inspection,
            Ok(_) | Err(_) => {
                if let Err(error) = state_db
                    .update_thread_selfwork_plan_path(thread_id, None)
                    .await
                {
                    tracing::warn!(thread_id = %thread_id, %error, "failed to clear invalid restored selfwork");
                }
                let status = {
                    let state = thread_state.lock().await;
                    selfwork_status(thread_id, &state)
                };
                self.broadcast_selfwork_updated(status).await;
                return;
            }
        };
        {
            let mut state = thread_state.lock().await;
            if state.selfwork.is_some() {
                return;
            }
            let mut runtime = praxis_app_core::selfwork::SelfworkRuntimeState::default();
            runtime.arm(&inspection);
            state.selfwork = Some(ThreadSelfworkState {
                plan_path: inspection.path,
                runtime,
                active_turn_id: None,
            });
        }
        if !matches!(thread.agent_status().await, AgentStatus::Running)
            && let Err(error) = start_selfwork_turn(&thread_state, thread).await
        {
            tracing::warn!(thread_id = %thread_id, %error, "failed to resume selfwork");
            thread_state.lock().await.selfwork = None;
            if let Err(clear_error) = state_db
                .update_thread_selfwork_plan_path(thread_id, None)
                .await
            {
                tracing::warn!(thread_id = %thread_id, %clear_error, "failed to clear selfwork after resume failure");
            }
            let status = {
                let state = thread_state.lock().await;
                selfwork_status(thread_id, &state)
            };
            self.broadcast_selfwork_updated(status).await;
            return;
        }
        let status = {
            let state = thread_state.lock().await;
            selfwork_status(thread_id, &state)
        };
        self.broadcast_selfwork_updated(status).await;
    }
}

pub(crate) async fn advance_selfwork_after_turn(
    conversation_id: ThreadId,
    completed_turn_id: &str,
    completed_status: &TurnStatus,
    conversation: &Arc<PraxisThread>,
    thread_state: &Arc<Mutex<ThreadState>>,
    outgoing: &ThreadScopedOutgoingMessageSender,
    state_db: Option<&Arc<StateRuntime>>,
) {
    let was_selfwork_turn = {
        let mut state = thread_state.lock().await;
        let Some(selfwork) = state.selfwork.as_mut() else {
            return;
        };
        match selfwork.active_turn_id.as_deref() {
            Some(active_turn_id) if active_turn_id == completed_turn_id => {
                selfwork.active_turn_id = None;
                selfwork.runtime.finish_turn();
                true
            }
            Some(_) => return,
            None => false,
        }
    };

    if was_selfwork_turn && !matches!(completed_status, TurnStatus::Completed) {
        stop_selfwork_after_turn(conversation_id, thread_state, outgoing, state_db).await;
        return;
    }

    if was_selfwork_turn {
        let plan_path = {
            let state = thread_state.lock().await;
            state
                .selfwork
                .as_ref()
                .map(|selfwork| selfwork.plan_path.clone())
        };
        let Some(plan_path) = plan_path else {
            return;
        };
        let inspection = match inspect_selfwork_plan(&plan_path) {
            Ok(inspection) => inspection,
            Err(_) => {
                stop_selfwork_after_turn(conversation_id, thread_state, outgoing, state_db).await;
                return;
            }
        };
        let advance = {
            let mut state = thread_state.lock().await;
            let Some(selfwork) = state.selfwork.as_mut() else {
                return;
            };
            selfwork.runtime.observe_plan_after_turn(&inspection)
        };
        if matches!(
            advance,
            SelfworkPlanAdvance::Complete | SelfworkPlanAdvance::Stalled { .. }
        ) {
            stop_selfwork_after_turn(conversation_id, thread_state, outgoing, state_db).await;
            return;
        }
    }

    if let Err(error) = start_selfwork_turn(thread_state, conversation).await {
        tracing::warn!(thread_id = %conversation_id, error, "failed to continue selfwork");
        stop_selfwork_after_turn(conversation_id, thread_state, outgoing, state_db).await;
        return;
    }
    let status = {
        let state = thread_state.lock().await;
        selfwork_status(conversation_id, &state)
    };
    outgoing
        .send_server_notification(ServerNotification::ThreadSelfworkUpdated(
            ThreadSelfworkUpdatedNotification { status },
        ))
        .await;
}

async fn start_selfwork_turn(
    thread_state: &Arc<Mutex<ThreadState>>,
    conversation: &Arc<PraxisThread>,
) -> Result<(), String> {
    let inspection = {
        let state = thread_state.lock().await;
        let Some(selfwork) = state.selfwork.as_ref() else {
            return Ok(());
        };
        if selfwork.runtime.turn_in_flight() || selfwork.active_turn_id.is_some() {
            return Ok(());
        }
        inspect_selfwork_plan(&selfwork.plan_path)?
    };
    if inspection.complete {
        return Err(format!(
            "selfwork plan is already complete: {}",
            inspection.path.display()
        ));
    }
    {
        let mut state = thread_state.lock().await;
        let Some(selfwork) = state.selfwork.as_mut() else {
            return Ok(());
        };
        selfwork.runtime.begin_turn(&inspection);
    }
    let result = conversation
        .submit_user_turn(
            vec![CoreUserInput::Text {
                text: selfwork_prompt(&inspection.path),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await;
    let mut state = thread_state.lock().await;
    let Some(selfwork) = state.selfwork.as_mut() else {
        return Ok(());
    };
    match result {
        Ok(turn_id) => {
            selfwork.active_turn_id = Some(turn_id);
            Ok(())
        }
        Err(error) => {
            selfwork.runtime.finish_turn();
            Err(format!("failed to submit selfwork turn: {error}"))
        }
    }
}

async fn stop_selfwork_after_turn(
    thread_id: ThreadId,
    thread_state: &Arc<Mutex<ThreadState>>,
    outgoing: &ThreadScopedOutgoingMessageSender,
    state_db: Option<&Arc<StateRuntime>>,
) {
    thread_state.lock().await.selfwork = None;
    if let Some(state_db) = state_db {
        if let Err(error) = state_db
            .update_thread_selfwork_plan_path(thread_id, None)
            .await
        {
            tracing::warn!(thread_id = %thread_id, %error, "failed to clear persisted selfwork");
        }
    }
    let status = {
        let state = thread_state.lock().await;
        selfwork_status(thread_id, &state)
    };
    outgoing
        .send_server_notification(ServerNotification::ThreadSelfworkUpdated(
            ThreadSelfworkUpdatedNotification { status },
        ))
        .await;
}

fn selfwork_status(thread_id: ThreadId, state: &ThreadState) -> ThreadSelfworkStatus {
    let Some(selfwork) = state.selfwork.as_ref() else {
        return ThreadSelfworkStatus {
            thread_id: thread_id.to_string(),
            phase: ThreadSelfworkPhase::Off,
            plan_path: None,
            stall_count: 0,
            stall_limit: SELFWORK_STALL_LIMIT,
        };
    };
    ThreadSelfworkStatus {
        thread_id: thread_id.to_string(),
        phase: if selfwork.runtime.turn_in_flight() {
            ThreadSelfworkPhase::Running
        } else {
            ThreadSelfworkPhase::Armed
        },
        plan_path: Some(selfwork.plan_path.clone()),
        stall_count: selfwork.runtime.stall_count(),
        stall_limit: SELFWORK_STALL_LIMIT,
    }
}
