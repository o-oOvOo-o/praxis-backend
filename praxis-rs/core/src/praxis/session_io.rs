use super::*;

impl Session {
    /// Inject additional user input into the currently active turn.
    ///
    /// Returns the active turn id when accepted.
    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
    ) -> Result<String, SteerInputError> {
        if input.is_empty() {
            return Err(SteerInputError::EmptyInput);
        }

        let mut active = self.active_turn.lock().await;
        let Some(active_turn) = active.as_mut() else {
            return Err(SteerInputError::NoActiveTurn(input));
        };

        let Some((active_turn_id, _)) = active_turn.tasks.first() else {
            return Err(SteerInputError::NoActiveTurn(input));
        };

        if let Some(expected_turn_id) = expected_turn_id
            && expected_turn_id != active_turn_id
        {
            return Err(SteerInputError::ExpectedTurnMismatch {
                expected: expected_turn_id.to_string(),
                actual: active_turn_id.clone(),
            });
        }

        match active_turn.tasks.first().map(|(_, task)| task.kind) {
            Some(crate::state::TaskKind::Regular) => {}
            Some(crate::state::TaskKind::Review) => {
                return Err(SteerInputError::ActiveTurnNotSteerable {
                    turn_kind: NonSteerableTurnKind::Review,
                });
            }
            Some(crate::state::TaskKind::Compact) => {
                return Err(SteerInputError::ActiveTurnNotSteerable {
                    turn_kind: NonSteerableTurnKind::Compact,
                });
            }
            None => return Err(SteerInputError::NoActiveTurn(input)),
        }

        let mut turn_state = active_turn.turn_state.lock().await;
        turn_state.push_pending_input(input.into());
        Ok(active_turn_id.clone())
    }

    /// Returns the input if there was no task running to inject into.
    pub async fn inject_response_items(
        &self,
        input: Vec<ResponseInputItem>,
    ) -> Result<(), Vec<ResponseInputItem>> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                for item in input {
                    ts.push_pending_input(item);
                }
                Ok(())
            }
            None => Err(input),
        }
    }

    pub(crate) fn subscribe_mailbox_seq(&self) -> watch::Receiver<u64> {
        self.mailbox.subscribe()
    }

    pub(crate) fn enqueue_mailbox_communication(&self, communication: InterAgentCommunication) {
        self.mailbox.send(communication);
    }

    pub(crate) async fn has_trigger_turn_mailbox_items(&self) -> bool {
        self.mailbox_rx.lock().await.has_pending_trigger_turn()
    }

    pub async fn prepend_pending_input(&self, input: Vec<ResponseInputItem>) -> Result<(), ()> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                ts.prepend_pending_input(input);
                Ok(())
            }
            None => Err(()),
        }
    }

    pub async fn get_pending_input(&self) -> Vec<ResponseInputItem> {
        let pending_input = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.take_pending_input()
                }
                None => Vec::new(),
            }
        };
        let runtime_command_items = self.claim_runtime_command_input_items().await;
        let mailbox_items = {
            let mut mailbox_rx = self.mailbox_rx.lock().await;
            mailbox_rx
                .drain()
                .into_iter()
                .map(|mail| mail.to_response_input_item())
                .collect::<Vec<_>>()
        };

        let mut combined = Vec::with_capacity(
            pending_input.len() + runtime_command_items.len() + mailbox_items.len(),
        );
        // Priority order matters: explicit pending input first, structured
        // AgentOS commands second, compatibility mailbox notifications last.
        // This makes RuntimeCommandBus the task source of truth while keeping
        // legacy mailbox delivery as a wake-up/notification channel.
        combined.extend(pending_input);
        combined.extend(runtime_command_items);
        combined.extend(mailbox_items);
        combined
    }

    async fn claim_runtime_command_input_items(&self) -> Vec<ResponseInputItem> {
        match self
            .services
            .agent_os
            .claim_runtime_commands_for_turn(self.conversation_id)
            .await
        {
            Ok(commands) => commands
                .into_iter()
                .map(Self::runtime_command_to_response_input_item)
                .collect(),
            Err(err) => {
                tracing::warn!(
                    %err,
                    thread_id = %self.conversation_id,
                    "failed to claim AgentOS runtime commands for turn"
                );
                Vec::new()
            }
        }
    }

    fn runtime_command_to_response_input_item(command: RuntimeCommandRecord) -> ResponseInputItem {
        let payload = serde_json::json!({
            "type": "agentos_runtime_command",
            "command_id": command.command_id,
            "command_type": command.command_type.as_str(),
            "task_id": command.task_id,
            "from_thread_id": command.from_thread_id.to_string(),
            "to_thread_id": command.to_thread_id.to_string(),
            "status": format!("{:?}", command.status),
            "payload": command.payload,
        });
        ResponseInputItem::Message {
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: serde_json::to_string(&payload).unwrap_or_default(),
            }],
        }
    }

    /// Queue response items to be injected into the next active turn created for this session.
    pub(crate) async fn queue_response_items_for_next_turn(&self, items: Vec<ResponseInputItem>) {
        if items.is_empty() {
            return;
        }

        let mut idle_pending_input = self.idle_pending_input.lock().await;
        idle_pending_input.extend(items);
    }

    pub(crate) async fn take_queued_response_items_for_next_turn(&self) -> Vec<ResponseInputItem> {
        std::mem::take(&mut *self.idle_pending_input.lock().await)
    }

    pub(crate) async fn has_queued_response_items_for_next_turn(&self) -> bool {
        !self.idle_pending_input.lock().await.is_empty()
    }

    pub async fn has_pending_input(&self) -> bool {
        if self.mailbox_rx.lock().await.has_pending() {
            return true;
        }
        if self
            .services
            .agent_os
            .has_claimable_runtime_command_for_thread(self.conversation_id)
            .await
        {
            return true;
        }
        let active = self.active_turn.lock().await;
        match active.as_ref() {
            Some(at) => {
                let ts = at.turn_state.lock().await;
                ts.has_pending_input()
            }
            None => false,
        }
    }

    pub(crate) async fn has_pending_input_bounded(&self, phase: &'static str) -> bool {
        match tokio::time::timeout(
            std::time::Duration::from_millis(PENDING_INPUT_CHECK_TIMEOUT_MS),
            self.has_pending_input(),
        )
        .await
        {
            Ok(has_pending) => has_pending,
            Err(_) => {
                warn!(
                    thread_id = %self.conversation_id,
                    phase,
                    "timed out checking pending input; assuming none"
                );
                false
            }
        }
    }

    pub async fn list_resources(
        &self,
        server: &str,
        params: Option<PaginatedRequestParams>,
    ) -> anyhow::Result<ListResourcesResult> {
        self.services
            .mcp_connection_manager
            .read()
            .await
            .list_resources(server, params)
            .await
    }

    pub async fn list_resource_templates(
        &self,
        server: &str,
        params: Option<PaginatedRequestParams>,
    ) -> anyhow::Result<ListResourceTemplatesResult> {
        self.services
            .mcp_connection_manager
            .read()
            .await
            .list_resource_templates(server, params)
            .await
    }

    pub async fn read_resource(
        &self,
        server: &str,
        params: ReadResourceRequestParams,
    ) -> anyhow::Result<ReadResourceResult> {
        self.services
            .mcp_connection_manager
            .read()
            .await
            .read_resource(server, params)
            .await
    }

    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
    ) -> anyhow::Result<CallToolResult> {
        self.services
            .mcp_connection_manager
            .read()
            .await
            .call_tool(server, tool, arguments, meta)
            .await
    }

    pub(crate) async fn parse_mcp_tool_name(
        &self,
        name: &str,
        namespace: &Option<String>,
    ) -> Option<(String, String)> {
        let tool_name = if let Some(namespace) = namespace {
            if name.starts_with(namespace.as_str()) {
                name
            } else {
                &format!("{namespace}{name}")
            }
        } else {
            name
        };
        self.services
            .mcp_connection_manager
            .read()
            .await
            .parse_tool_name(tool_name)
            .await
    }

    pub async fn interrupt_task(self: &Arc<Self>) {
        info!("interrupt received: abort current task, if any");
        let has_active_turn = { self.active_turn.lock().await.is_some() };
        if has_active_turn {
            self.abort_all_tasks(TurnAbortReason::Interrupted).await;
        } else {
            self.cancel_mcp_startup().await;
        }
    }
}
