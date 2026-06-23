use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplayKind {
    ResumeInitialMessages,
    ThreadSnapshot,
}

impl ReplayKind {
    pub(super) fn preserves_live_running_state(self) -> bool {
        matches!(self, Self::ThreadSnapshot)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThreadItemRenderSource {
    Live,
    Replay(ReplayKind),
}

impl ThreadItemRenderSource {
    pub(super) fn is_replay(self) -> bool {
        matches!(self, Self::Replay(_))
    }

    pub(super) fn replay_kind(self) -> Option<ReplayKind> {
        match self {
            Self::Live => None,
            Self::Replay(replay_kind) => Some(replay_kind),
        }
    }
}

pub(super) fn session_state_to_configured_event(
    session: ThreadSessionState,
) -> praxis_protocol::protocol::SessionConfiguredEvent {
    praxis_protocol::protocol::SessionConfiguredEvent {
        session_id: session.thread_id,
        forked_from_id: session.forked_from_id,
        thread_name: session.thread_name,
        model: session.model,
        model_provider_id: session.model_provider_id,
        service_tier: session.service_tier,
        approval_policy: session.approval_policy,
        approvals_reviewer: session.approvals_reviewer,
        sandbox_policy: session.sandbox_policy,
        cwd: session.cwd,
        reasoning_effort: session.reasoning_effort,
        history_log_id: session.history_log_id,
        history_entry_count: usize::try_from(session.history_entry_count).unwrap_or(usize::MAX),
        initial_messages: None,
        network_proxy: session.network_proxy,
        rollout_path: session.rollout_path,
    }
}

pub(super) fn hook_output_entry_from_notification(
    entry: praxis_app_gateway_protocol::HookOutputEntry,
) -> praxis_protocol::protocol::HookOutputEntry {
    praxis_protocol::protocol::HookOutputEntry {
        kind: entry.kind.to_core(),
        text: entry.text,
    }
}

pub(super) fn hook_run_summary_from_notification(
    run: praxis_app_gateway_protocol::HookRunSummary,
) -> praxis_protocol::protocol::HookRunSummary {
    praxis_protocol::protocol::HookRunSummary {
        id: run.id,
        event_name: run.event_name.to_core(),
        handler_type: run.handler_type.to_core(),
        execution_mode: run.execution_mode.to_core(),
        scope: run.scope.to_core(),
        source_path: run.source_path,
        display_order: run.display_order,
        status: run.status.to_core(),
        status_message: run.status_message,
        started_at: run.started_at,
        completed_at: run.completed_at,
        duration_ms: run.duration_ms,
        entries: run
            .entries
            .into_iter()
            .map(hook_output_entry_from_notification)
            .collect(),
    }
}

pub(super) fn hook_started_event_from_notification(
    notification: praxis_app_gateway_protocol::HookStartedNotification,
) -> praxis_protocol::protocol::HookStartedEvent {
    praxis_protocol::protocol::HookStartedEvent {
        turn_id: notification.turn_id,
        run: hook_run_summary_from_notification(notification.run),
    }
}

pub(super) fn hook_completed_event_from_notification(
    notification: praxis_app_gateway_protocol::HookCompletedNotification,
) -> praxis_protocol::protocol::HookCompletedEvent {
    praxis_protocol::protocol::HookCompletedEvent {
        turn_id: notification.turn_id,
        run: hook_run_summary_from_notification(notification.run),
    }
}

/// Converts app-gateway collab agent states into the core protocol representation, enriching each
/// entry with cached nickname and role metadata so rendered items show human-readable names.
pub(super) fn app_gateway_collab_agent_statuses_to_core(
    receiver_thread_ids: &[String],
    agents_states: &HashMap<String, AppGatewayCollabAgentState>,
    collab_agent_metadata: &HashMap<ThreadId, CollabAgentMetadata>,
) -> (Vec<CollabAgentStatusEntry>, HashMap<ThreadId, AgentStatus>) {
    let mut agent_statuses = Vec::new();
    let mut statuses = HashMap::new();

    for receiver_thread_id in receiver_thread_ids {
        let Some(thread_id) = app_gateway_collab_thread_id_to_core(receiver_thread_id) else {
            continue;
        };
        let Some(agent_state) = agents_states.get(receiver_thread_id) else {
            continue;
        };
        let status = app_gateway_collab_state_to_core(agent_state);
        let metadata = collab_agent_metadata
            .get(&thread_id)
            .cloned()
            .unwrap_or_default();
        agent_statuses.push(CollabAgentStatusEntry {
            thread_id,
            agent_base_name: metadata.agent_base_name,
            agent_title: metadata.agent_title,
            agent_display_name: metadata.agent_display_name,
            agent_role: metadata.agent_role,
            status: status.clone(),
        });
        statuses.insert(thread_id, status);
    }

    (agent_statuses, statuses)
}

/// Builds `CollabAgentRef` entries for every valid receiver thread, attaching cached metadata.
///
/// Used when converting collab `Wait` tool-call items so the rendered waiting list shows agent
/// names instead of bare thread ids.
pub(super) fn app_gateway_collab_receiver_agent_refs(
    receiver_thread_ids: &[String],
    collab_agent_metadata: &HashMap<ThreadId, CollabAgentMetadata>,
) -> Vec<CollabAgentRef> {
    receiver_thread_ids
        .iter()
        .filter_map(|thread_id| {
            let thread_id = app_gateway_collab_thread_id_to_core(thread_id)?;
            let metadata = collab_agent_metadata
                .get(&thread_id)
                .cloned()
                .unwrap_or_default();
            Some(CollabAgentRef {
                thread_id,
                agent_base_name: metadata.agent_base_name,
                agent_title: metadata.agent_title,
                agent_display_name: metadata.agent_display_name,
                agent_role: metadata.agent_role,
            })
        })
        .collect()
}
