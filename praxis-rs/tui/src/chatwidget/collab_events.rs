use super::*;

impl ChatWidget {
    pub(super) fn on_collab_event(&mut self, cell: PlainHistoryCell) {
        self.flush_answer_stream_with_separator();
        self.add_to_history(cell);
        self.request_redraw();
    }

    pub(super) fn on_collab_agent_tool_call(&mut self, item: ThreadItem) {
        let ThreadItem::CollabAgentToolCall {
            id,
            tool,
            status,
            sender_thread_id,
            receiver_thread_ids,
            prompt,
            model,
            reasoning_effort,
            agents_states,
        } = item
        else {
            return;
        };
        let sender_thread_id = app_gateway_collab_thread_id_to_core(&sender_thread_id)
            .or(self.thread_id)
            .unwrap_or_default();
        let first_receiver = receiver_thread_ids
            .first()
            .and_then(|thread_id| app_gateway_collab_thread_id_to_core(thread_id));
        let first_receiver_metadata =
            first_receiver.map(|thread_id| self.collab_agent_metadata(thread_id));

        match tool {
            CollabAgentTool::SpawnAgent => {
                if let (Some(model), Some(reasoning_effort)) =
                    (model.clone(), reasoning_effort.clone())
                {
                    self.pending_collab_spawn_requests.insert(
                        id.clone(),
                        multi_agents::SpawnRequestSummary {
                            model,
                            reasoning_effort,
                        },
                    );
                }

                if !matches!(status, CollabAgentToolCallStatus::InProgress) {
                    let spawn_request =
                        self.pending_collab_spawn_requests.remove(&id).or_else(|| {
                            model
                                .zip(reasoning_effort.clone())
                                .map(|(model, reasoning_effort)| {
                                    multi_agents::SpawnRequestSummary {
                                        model,
                                        reasoning_effort,
                                    }
                                })
                        });
                    self.on_collab_event(multi_agents::spawn_end(
                        praxis_protocol::protocol::CollabAgentSpawnEndEvent {
                            call_id: id,
                            sender_thread_id,
                            new_thread_id: first_receiver,
                            new_agent_base_name: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_base_name.clone()),
                            new_agent_title: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_title.clone()),
                            new_agent_display_name: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_display_name.clone()),
                            new_agent_role: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_role.clone()),
                            prompt: prompt.unwrap_or_default(),
                            model: String::new(),
                            reasoning_effort: ReasoningEffortConfig::Medium,
                            status: first_receiver
                                .as_ref()
                                .and_then(|thread_id| agents_states.get(&thread_id.to_string()))
                                .map(app_gateway_collab_state_to_core)
                                .unwrap_or_else(|| {
                                    AgentStatus::Errored("Agent spawn failed".into())
                                }),
                        },
                        spawn_request.as_ref(),
                    ));
                }
            }
            tool @ (CollabAgentTool::SendMessage | CollabAgentTool::AssignTask) => {
                if let Some(receiver_thread_id) = first_receiver
                    && !matches!(status, CollabAgentToolCallStatus::InProgress)
                {
                    let kind = match tool {
                        CollabAgentTool::SendMessage => {
                            praxis_protocol::protocol::CollabAgentInteractionKind::SendMessage
                        }
                        CollabAgentTool::AssignTask => {
                            praxis_protocol::protocol::CollabAgentInteractionKind::AssignTask
                        }
                        _ => unreachable!(),
                    };
                    self.on_collab_event(multi_agents::interaction_end(
                        praxis_protocol::protocol::CollabAgentInteractionEndEvent {
                            call_id: id,
                            sender_thread_id,
                            receiver_thread_id,
                            kind,
                            receiver_agent_base_name: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_base_name.clone()),
                            receiver_agent_title: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_title.clone()),
                            receiver_agent_display_name: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_display_name.clone()),
                            receiver_agent_role: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_role.clone()),
                            prompt: prompt.unwrap_or_default(),
                            status: receiver_thread_ids
                                .iter()
                                .find_map(|thread_id| agents_states.get(thread_id))
                                .map(app_gateway_collab_state_to_core)
                                .unwrap_or_else(|| {
                                    AgentStatus::Errored("Agent interaction failed".into())
                                }),
                        },
                    ));
                }
            }
            CollabAgentTool::ResumeThread => {
                if let Some(receiver_thread_id) = first_receiver {
                    if matches!(status, CollabAgentToolCallStatus::InProgress) {
                        self.on_collab_event(multi_agents::resume_begin(
                            praxis_protocol::protocol::CollabResumeBeginEvent {
                                call_id: id,
                                sender_thread_id,
                                receiver_thread_id,
                                receiver_agent_base_name: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_base_name.clone()),
                                receiver_agent_title: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_title.clone()),
                                receiver_agent_display_name: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_display_name.clone()),
                                receiver_agent_role: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_role.clone()),
                            },
                        ));
                    } else {
                        self.on_collab_event(multi_agents::resume_end(
                            praxis_protocol::protocol::CollabResumeEndEvent {
                                call_id: id,
                                sender_thread_id,
                                receiver_thread_id,
                                receiver_agent_base_name: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_base_name.clone()),
                                receiver_agent_title: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_title.clone()),
                                receiver_agent_display_name: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_display_name.clone()),
                                receiver_agent_role: first_receiver_metadata
                                    .as_ref()
                                    .and_then(|metadata| metadata.agent_role.clone()),
                                status: receiver_thread_ids
                                    .iter()
                                    .find_map(|thread_id| agents_states.get(thread_id))
                                    .map(app_gateway_collab_state_to_core)
                                    .unwrap_or_else(|| {
                                        AgentStatus::Errored("Agent resume failed".into())
                                    }),
                            },
                        ));
                    }
                }
            }
            CollabAgentTool::Wait => {
                if matches!(status, CollabAgentToolCallStatus::InProgress) {
                    self.on_collab_event(multi_agents::waiting_begin(
                        praxis_protocol::protocol::CollabWaitingBeginEvent {
                            sender_thread_id,
                            receiver_thread_ids: receiver_thread_ids
                                .iter()
                                .filter_map(|thread_id| {
                                    app_gateway_collab_thread_id_to_core(thread_id)
                                })
                                .collect(),
                            receiver_agents: app_gateway_collab_receiver_agent_refs(
                                &receiver_thread_ids,
                                &self.collab_agent_metadata,
                            ),
                            call_id: id,
                        },
                    ));
                } else {
                    let (agent_statuses, statuses) = app_gateway_collab_agent_statuses_to_core(
                        &receiver_thread_ids,
                        &agents_states,
                        &self.collab_agent_metadata,
                    );
                    self.on_collab_event(multi_agents::waiting_end(
                        praxis_protocol::protocol::CollabWaitingEndEvent {
                            sender_thread_id,
                            call_id: id,
                            agent_statuses,
                            statuses,
                        },
                    ));
                }
            }
            CollabAgentTool::CloseAgent => {
                if let Some(receiver_thread_id) = first_receiver
                    && !matches!(status, CollabAgentToolCallStatus::InProgress)
                {
                    self.on_collab_event(multi_agents::close_end(
                        praxis_protocol::protocol::CollabCloseEndEvent {
                            call_id: id,
                            sender_thread_id,
                            receiver_thread_id,
                            receiver_agent_base_name: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_base_name.clone()),
                            receiver_agent_title: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_title.clone()),
                            receiver_agent_display_name: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_display_name.clone()),
                            receiver_agent_role: first_receiver_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.agent_role.clone()),
                            status: receiver_thread_ids
                                .iter()
                                .find_map(|thread_id| agents_states.get(thread_id))
                                .map(app_gateway_collab_state_to_core)
                                .unwrap_or_else(|| {
                                    AgentStatus::Errored("Agent close failed".into())
                                }),
                        },
                    ));
                }
            }
        }
    }
}
