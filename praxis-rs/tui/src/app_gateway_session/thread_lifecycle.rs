use super::*;

impl AppGatewaySession {
    pub(crate) async fn start_thread(
        &mut self,
        config: &Config,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadStartResponse = self
            .client
            .request_typed(ClientRequest::ThreadStart {
                request_id,
                params: thread_start_params_from_config(config, self.thread_params_mode()),
            })
            .await
            .wrap_err("thread/start failed during TUI bootstrap")?;
        started_thread_from_start_response(response, config).await
    }

    pub(crate) async fn resume_thread(
        &mut self,
        config: Config,
        thread_id: ThreadId,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadResumeResponse = self
            .client
            .request_typed(ClientRequest::ThreadResume {
                request_id,
                params: thread_resume_params_from_config(
                    config.clone(),
                    thread_id,
                    self.thread_params_mode(),
                ),
            })
            .await
            .wrap_err("thread/resume failed during TUI bootstrap")?;
        started_thread_from_resume_response(response, &config).await
    }

    pub(crate) async fn watch_thread(
        &mut self,
        config: &Config,
        thread_id: ThreadId,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadResumeResponse = self
            .client
            .request_typed(ClientRequest::ThreadResume {
                request_id,
                params: thread_resume_params_from_config(
                    config.clone(),
                    thread_id,
                    self.thread_params_mode(),
                ),
            })
            .await
            .wrap_err("thread/resume watch failed in TUI")?;
        started_thread_from_resume_response(response, config).await
    }

    pub(crate) async fn fork_thread(
        &mut self,
        config: Config,
        thread_id: ThreadId,
        path: Option<PathBuf>,
    ) -> Result<AppGatewayStartedThread> {
        let request_id = self.next_request_id();
        let response: ThreadForkResponse = self
            .client
            .request_typed(ClientRequest::ThreadFork {
                request_id,
                params: thread_fork_params_from_config(
                    config.clone(),
                    thread_id,
                    path,
                    self.thread_params_mode(),
                ),
            })
            .await
            .wrap_err("thread/fork failed during TUI bootstrap")?;
        started_thread_from_fork_response(response, &config).await
    }

    fn thread_params_mode(&self) -> ThreadParamsMode {
        match &self.client {
            AppGatewayClient::Native(_) => ThreadParamsMode::Embedded,
            AppGatewayClient::Remote(_) => ThreadParamsMode::Remote,
        }
    }

    pub(crate) async fn thread_list(
        &mut self,
        params: ThreadListParams,
    ) -> Result<ThreadListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadList { request_id, params })
            .await
            .wrap_err("thread/list failed during TUI session lookup")
    }

    pub(crate) async fn thread_lookup(
        &mut self,
        params: ThreadLookupParams,
    ) -> Result<Option<Thread>> {
        let request_id = self.next_request_id();
        let response: ThreadLookupResponse = self
            .client
            .request_typed(ClientRequest::ThreadLookup { request_id, params })
            .await
            .wrap_err("thread/lookup failed during TUI session lookup")?;
        Ok(response.thread)
    }

    /// Lists thread ids that the app gateway currently holds in memory.
    ///
    /// Used by `App::backfill_loaded_subagent_threads` to discover subagent threads that were
    /// spawned before the TUI connected. The caller then fetches full metadata per thread via
    /// `thread_read` and walks the spawn tree.
    pub(crate) async fn thread_loaded_list(
        &mut self,
        params: ThreadLoadedListParams,
    ) -> Result<ThreadLoadedListResponse> {
        let request_id = self.next_request_id();
        self.client
            .request_typed(ClientRequest::ThreadLoadedList { request_id, params })
            .await
            .wrap_err("failed to list loaded threads from app gateway")
    }

    pub(crate) async fn thread_read(
        &mut self,
        thread_id: ThreadId,
        include_turns: bool,
    ) -> Result<Thread> {
        let request_id = self.next_request_id();
        let response: ThreadReadResponse = self
            .client
            .request_typed(ClientRequest::ThreadRead {
                request_id,
                params: ThreadReadParams {
                    thread_id: thread_id.to_string(),
                    include_turns,
                    turn_limit: include_turns.then_some(THREAD_TURN_HYDRATION_LIMIT),
                },
            })
            .await
            .wrap_err("thread/read failed during TUI session lookup")?;
        Ok(response.thread)
    }

    pub(crate) async fn thread_goal_get(
        &mut self,
        thread_id: ThreadId,
    ) -> Result<Option<ThreadGoal>> {
        let request_id = self.next_request_id();
        let response: ThreadGoalGetResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalGet {
                request_id,
                params: ThreadGoalGetParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/goal/get failed in TUI")?;
        Ok(response.goal)
    }

    pub(crate) async fn thread_goal_set(
        &mut self,
        thread_id: ThreadId,
        objective: String,
    ) -> Result<ThreadGoal> {
        let request_id = self.next_request_id();
        let response: ThreadGoalSetResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalSet {
                request_id,
                params: ThreadGoalSetParams {
                    thread_id: thread_id.to_string(),
                    objective,
                    token_budget: None,
                    clear_token_budget: false,
                },
            })
            .await
            .wrap_err("thread/goal/set failed in TUI")?;
        Ok(response.goal)
    }

    pub(crate) async fn thread_goal_update(
        &mut self,
        thread_id: ThreadId,
        objective: Option<String>,
        status: Option<ThreadGoalStatus>,
        token_budget: Option<i64>,
        clear_token_budget: bool,
    ) -> Result<ThreadGoal> {
        let request_id = self.next_request_id();
        let response: ThreadGoalUpdateResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalUpdate {
                request_id,
                params: ThreadGoalUpdateParams {
                    thread_id: thread_id.to_string(),
                    objective,
                    status,
                    token_budget,
                    clear_token_budget,
                },
            })
            .await
            .wrap_err("thread/goal/update failed in TUI")?;
        Ok(response.goal)
    }

    pub(crate) async fn thread_goal_clear(&mut self, thread_id: ThreadId) -> Result<bool> {
        let request_id = self.next_request_id();
        let response: ThreadGoalClearResponse = self
            .client
            .request_typed(ClientRequest::ThreadGoalClear {
                request_id,
                params: ThreadGoalClearParams {
                    thread_id: thread_id.to_string(),
                },
            })
            .await
            .wrap_err("thread/goal/clear failed in TUI")?;
        Ok(response.cleared)
    }
}
