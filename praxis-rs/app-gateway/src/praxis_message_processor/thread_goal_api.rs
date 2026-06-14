use super::*;

impl PraxisMessageProcessor {
    pub(crate) async fn thread_goal_get(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadGoalGetParams,
    ) {
        let thread = match self
            .goal_thread_for_request(request_id.clone(), &params.thread_id)
            .await
        {
            Some(thread) => thread,
            None => return,
        };
        match thread.get_thread_goal().await {
            Ok(goal) => {
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadGoalGetResponse {
                            goal: goal.map(Into::into),
                        },
                    )
                    .await;
            }
            Err(err) => {
                self.send_internal_error(request_id, format!("failed to read thread goal: {err}"))
                    .await;
            }
        }
    }

    pub(crate) async fn thread_goal_set(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadGoalSetParams,
    ) {
        let thread_id = params.thread_id.clone();
        let thread = match self
            .goal_thread_for_request(request_id.clone(), &thread_id)
            .await
        {
            Some(thread) => thread,
            None => return,
        };
        let token_budget = token_budget_update(params.token_budget, params.clear_token_budget);
        match thread
            .set_thread_goal_from_user(params.objective, token_budget)
            .await
        {
            Ok(goal) => {
                let goal = praxis_app_gateway_protocol::ThreadGoal::from(goal);
                self.outgoing
                    .send_response(request_id, ThreadGoalSetResponse { goal: goal.clone() })
                    .await;
                self.broadcast_goal_updated(thread_id, goal).await;
            }
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to set thread goal: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn thread_goal_update(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadGoalUpdateParams,
    ) {
        let thread_id = params.thread_id.clone();
        let thread = match self
            .goal_thread_for_request(request_id.clone(), &thread_id)
            .await
        {
            Some(thread) => thread,
            None => return,
        };
        let status = params.status.map(ThreadGoalStatus::to_core);
        let token_budget = token_budget_update(params.token_budget, params.clear_token_budget);
        match thread
            .update_thread_goal_from_user(params.objective, status, token_budget)
            .await
        {
            Ok(goal) => {
                let goal = praxis_app_gateway_protocol::ThreadGoal::from(goal);
                self.outgoing
                    .send_response(request_id, ThreadGoalUpdateResponse { goal: goal.clone() })
                    .await;
                self.broadcast_goal_updated(thread_id, goal).await;
            }
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to update thread goal: {err}"),
                )
                .await;
            }
        }
    }

    pub(crate) async fn thread_goal_clear(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadGoalClearParams,
    ) {
        let thread_id = params.thread_id.clone();
        let thread = match self
            .goal_thread_for_request(request_id.clone(), &thread_id)
            .await
        {
            Some(thread) => thread,
            None => return,
        };
        match thread.clear_thread_goal_from_user().await {
            Ok(cleared) => {
                self.outgoing
                    .send_response(request_id, ThreadGoalClearResponse { cleared })
                    .await;
                if cleared {
                    self.outgoing
                        .send_server_notification(ServerNotification::ThreadGoalCleared(
                            ThreadGoalClearedNotification { thread_id },
                        ))
                        .await;
                }
            }
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("failed to clear thread goal: {err}"),
                )
                .await;
            }
        }
    }

    async fn goal_thread_for_request(
        &self,
        request_id: ConnectionRequestId,
        thread_id: &str,
    ) -> Option<Arc<PraxisThread>> {
        let thread_uuid = match self.parse_thread_id(thread_id) {
            Ok(id) => id,
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return None;
            }
        };
        match self.thread_manager.get_thread(thread_uuid).await {
            Ok(thread) => Some(thread),
            Err(err) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("thread not loaded: {thread_uuid}: {err}"),
                )
                .await;
                None
            }
        }
    }

    async fn broadcast_goal_updated(
        &self,
        thread_id: String,
        goal: praxis_app_gateway_protocol::ThreadGoal,
    ) {
        self.outgoing
            .send_server_notification(ServerNotification::ThreadGoalUpdated(
                ThreadGoalUpdatedNotification {
                    thread_id,
                    turn_id: None,
                    goal,
                },
            ))
            .await;
    }
}

fn token_budget_update(token_budget: Option<i64>, clear_token_budget: bool) -> Option<Option<i64>> {
    if clear_token_budget {
        Some(None)
    } else {
        token_budget.map(Some)
    }
}
