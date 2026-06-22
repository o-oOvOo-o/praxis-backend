use tracing::debug;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

use crate::agent::AgentStatus;
use crate::agent::agent_status_from_event;
use crate::agent::status::is_final;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::session_prefix::format_subagent_notification_message;

impl Session {
    /// Forwards terminal turn events from spawned children to their direct parent.
    pub(in crate::praxis) async fn maybe_notify_parent_of_terminal_turn(
        &self,
        turn_context: &TurnContext,
        msg: &EventMsg,
    ) {
        if !matches!(msg, EventMsg::TurnComplete(_) | EventMsg::TurnAborted(_)) {
            return;
        }

        let SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            agent_path: Some(child_agent_path),
            ..
        }) = &turn_context.session_source
        else {
            return;
        };

        let Some(status) = agent_status_from_event(msg) else {
            return;
        };
        if !is_final(&status) {
            return;
        }

        self.forward_child_completion_to_parent(*parent_thread_id, child_agent_path, status)
            .await;
    }

    /// Sends the standard completion envelope from a spawned child to its parent.
    pub(in crate::praxis) async fn forward_child_completion_to_parent(
        &self,
        parent_thread_id: ThreadId,
        child_agent_path: &praxis_protocol::AgentPath,
        status: AgentStatus,
    ) {
        let Some(parent_agent_path) = child_agent_path
            .as_str()
            .rsplit_once('/')
            .and_then(|(parent, _)| praxis_protocol::AgentPath::try_from(parent).ok())
        else {
            return;
        };

        let message = format_subagent_notification_message(child_agent_path.as_str(), &status);
        let communication = InterAgentCommunication::new(
            child_agent_path.clone(),
            parent_agent_path,
            Vec::new(),
            message,
            false,
        );
        if let Err(err) = self
            .services
            .agent_control
            .send_inter_agent_communication(parent_thread_id, communication)
            .await
        {
            debug!("failed to notify parent thread {parent_thread_id}: {err}");
        }
    }
}
