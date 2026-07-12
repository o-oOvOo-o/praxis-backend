use super::thread_list::ThreadListFilters;
use super::*;
use crate::praxis_message_processor::thread_store_api::ThreadHistoryPageReadError;
use praxis_app_gateway_protocol::THREAD_LIST_MAX_LIMIT;
use praxis_core::ThreadSortKey as CoreThreadSortKey;

impl PraxisMessageProcessor {
    pub(in crate::praxis_message_processor) async fn thread_history_read(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadHistoryReadParams,
    ) {
        let ThreadHistoryReadParams {
            thread_id,
            cursor,
            limit,
        } = params;
        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        let preferred_path = self
            .thread_manager
            .get_thread(thread_uuid)
            .await
            .ok()
            .and_then(|thread| thread.rollout_path());
        let rollout_path =
            match resolve_thread_rollout_path(&self.config, thread_uuid, preferred_path).await {
                Ok(path) => path,
                Err(error) => {
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            };

        let page =
            match ThreadStore::read_turn_page_from_rollout(&rollout_path, cursor, limit).await {
                Ok(page) => page,
                Err(error @ ThreadHistoryPageReadError::InvalidCursor { .. })
                | Err(error @ ThreadHistoryPageReadError::InvalidLimit { .. }) => {
                    self.send_invalid_request_error(
                        request_id,
                        format!("invalid history page: {error}"),
                    )
                    .await;
                    return;
                }
                Err(ThreadHistoryPageReadError::Io(error)) => {
                    self.send_internal_error(
                        request_id,
                        format!(
                            "failed to load rollout `{}` for thread {thread_uuid}: {error}",
                            rollout_path.display()
                        ),
                    )
                    .await;
                    return;
                }
            };

        self.outgoing
            .send_response(
                request_id,
                ThreadHistoryReadResponse {
                    thread_id: thread_uuid.to_string(),
                    turns: page.turns,
                    page: page.page,
                },
            )
            .await;
    }

    pub(in crate::praxis_message_processor) async fn thread_read(
        &mut self,
        request_id: ConnectionRequestId,
        params: ThreadReadParams,
    ) {
        let ThreadReadParams {
            thread_id,
            include_turns,
            turn_limit,
        } = params;

        let Some(thread_uuid) = self
            .ensure_thread_id_for_request(&thread_id, &request_id)
            .await
        else {
            return;
        };

        let thread = match self
            .load_thread_for_projection(thread_uuid, include_turns, turn_limit)
            .await
        {
            Ok(Some(thread)) => thread,
            Ok(None) => {
                self.send_invalid_request_error(
                    request_id,
                    format!("thread not loaded: {thread_uuid}"),
                )
                .await;
                return;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        let response = ThreadReadResponse { thread };
        self.outgoing.send_response(request_id, response).await;
    }

    pub(in crate::praxis_message_processor) async fn thread_lookup(
        &self,
        request_id: ConnectionRequestId,
        params: ThreadLookupParams,
    ) {
        let response = match self.lookup_thread(params).await {
            Ok(thread) => ThreadLookupResponse { thread },
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };
        self.outgoing.send_response(request_id, response).await;
    }

    async fn lookup_thread(
        &self,
        params: ThreadLookupParams,
    ) -> Result<Option<Thread>, JSONRPCErrorError> {
        let ThreadLookupParams {
            selector,
            include_turns,
            turn_limit,
            source_kinds,
            cwd_scope,
            archived,
        } = params;

        match selector {
            ThreadLookupSelector::IdOrName { value } => {
                if let Ok(thread_id) = ThreadId::from_string(&value) {
                    return self
                        .load_thread_for_projection(thread_id, include_turns, turn_limit)
                        .await;
                }
                self.lookup_thread_from_store_pages(
                    Some(value.as_str()),
                    include_turns,
                    turn_limit,
                    source_kinds,
                    cwd_scope,
                    archived.unwrap_or(false),
                )
                .await
            }
            ThreadLookupSelector::Latest => {
                self.lookup_thread_from_store_pages(
                    None,
                    include_turns,
                    turn_limit,
                    source_kinds,
                    cwd_scope,
                    archived.unwrap_or(false),
                )
                .await
            }
        }
    }

    async fn lookup_thread_from_store_pages(
        &self,
        exact_name: Option<&str>,
        include_turns: bool,
        turn_limit: Option<u32>,
        source_kinds: Option<Vec<ApiThreadSourceKind>>,
        cwd_scope: Option<String>,
        archived: bool,
    ) -> Result<Option<Thread>, JSONRPCErrorError> {
        let scope_root = cwd_scope
            .as_deref()
            .map(|cwd| project_scope_root(Path::new(cwd)));
        let mut cursor = None;
        loop {
            let (summaries, next_cursor) = self
                .list_threads_common(
                    THREAD_LIST_MAX_LIMIT as usize,
                    cursor,
                    CoreThreadSortKey::UpdatedAt,
                    ThreadListFilters {
                        model_providers: None,
                        source_kinds: source_kinds.clone(),
                        archived,
                        cwd: None,
                        cwd_scope: scope_root.clone(),
                        search_term: None,
                    },
                )
                .await?;

            for summary in summaries {
                if let Some(expected_name) = exact_name
                    && summary.thread_name.as_deref() != Some(expected_name)
                {
                    continue;
                }
                if let Some(scope_root) = scope_root.as_ref()
                    && project_scope_root(summary.cwd.as_path()) != *scope_root
                {
                    continue;
                }
                if let Some(thread) = self
                    .load_thread_for_projection(summary.conversation_id, include_turns, turn_limit)
                    .await?
                {
                    return Ok(Some(thread));
                }
            }

            let Some(next_cursor) = next_cursor else {
                return Ok(None);
            };
            cursor = Some(next_cursor);
        }
    }

    pub(in crate::praxis_message_processor) async fn load_thread_for_projection(
        &self,
        thread_uuid: ThreadId,
        include_turns: bool,
        turn_limit: Option<u32>,
    ) -> Result<Option<Thread>, JSONRPCErrorError> {
        let loaded_thread = self.thread_manager.get_thread(thread_uuid).await.ok();
        let directory_summary = match ThreadStore::new(&self.config)
            .try_read_directory_summary(thread_uuid)
            .await
        {
            Ok(summary) => summary,
            Err(err)
                if loaded_thread.is_some()
                    && directory_summary_error_allows_live_snapshot(&err) =>
            {
                tracing::debug!(
                    "using live thread snapshot for {thread_uuid} after transient summary read error: {err}"
                );
                None
            }
            Err(err) => {
                return Err(JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to read thread {thread_uuid}: {err}"),
                    data: None,
                });
            }
        };
        let mut rollout_path = directory_summary
            .as_ref()
            .map(|summary| summary.path.clone());

        let mut thread = if let Some(summary) = directory_summary {
            summary_to_thread(summary)
        } else {
            let Some(thread) = loaded_thread.as_ref() else {
                return Ok(None);
            };
            let config_snapshot = thread.config_snapshot().await;
            let loaded_rollout_path = thread.rollout_path();
            if include_turns && loaded_rollout_path.is_none() {
                return Err(JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "ephemeral threads do not support includeTurns".to_string(),
                    data: None,
                });
            }
            if include_turns {
                rollout_path = loaded_rollout_path.clone();
            }
            build_thread_from_snapshot(thread_uuid, &config_snapshot, loaded_rollout_path)
        };
        if thread.name.is_none() {
            self.attach_thread_name(thread_uuid, &mut thread).await;
        }

        if include_turns && let Some(rollout_path) = rollout_path.as_ref() {
            match ThreadStore::read_turns_from_rollout(
                rollout_path,
                ThreadTurnHydration::recent(turn_limit.map(|limit| limit as usize)),
            )
            .await
            {
                Ok(turns) => {
                    thread.turns = turns;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return Err(JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!(
                            "thread {thread_uuid} is not materialized yet; includeTurns is unavailable before first user message"
                        ),
                        data: None,
                    });
                }
                Err(err) => {
                    return Err(JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!(
                            "failed to load rollout `{}` for thread {thread_uuid}: {err}",
                            rollout_path.display()
                        ),
                        data: None,
                    });
                }
            }
        }

        let has_live_in_progress_turn = if let Some(loaded_thread) = loaded_thread.as_ref() {
            matches!(loaded_thread.agent_status().await, AgentStatus::Running)
        } else {
            false
        };

        self.project_thread_runtime_state_with_turn_cleanup(&mut thread, has_live_in_progress_turn)
            .await;
        Ok(Some(thread))
    }
}

fn directory_summary_error_allows_live_snapshot(err: &std::io::Error) -> bool {
    err.to_string().contains("empty session file")
}
