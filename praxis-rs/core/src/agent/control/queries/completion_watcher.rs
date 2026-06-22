use super::super::*;
use crate::agent::status::is_final;
use crate::session_prefix::format_subagent_notification_message;

impl AgentControl {
    pub(crate) fn maybe_start_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        session_source: Option<SessionSource>,
        child_reference: String,
        child_agent_path: Option<AgentPath>,
    ) {
        let Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        })) = session_source
        else {
            return;
        };

        let control = self.clone();
        tokio::spawn(async move {
            let status = match control.subscribe_status(child_thread_id).await {
                Ok(mut status_rx) => loop {
                    let status = status_rx.borrow().clone();
                    if is_final(&status) {
                        break status;
                    }
                    if status_rx.changed().await.is_err() {
                        break AgentStatus::NotFound;
                    }
                },
                Err(PraxisErr::ThreadNotFound(_)) => AgentStatus::NotFound,
                Err(err) => AgentStatus::Errored(err.to_string()),
            };

            let author = child_agent_path
                .clone()
                .or_else(|| AgentPath::try_from(child_reference.as_str()).ok())
                .unwrap_or_else(AgentPath::root);
            let recipient = parent_agent_path_from_child_path(child_agent_path.as_ref())
                .unwrap_or_else(AgentPath::root);
            let message = format_subagent_notification_message(child_reference.as_str(), &status);
            let communication = InterAgentCommunication::new(
                author,
                recipient,
                Vec::new(),
                message,
                /*trigger_turn*/ false,
            );
            let _ = control
                .send_inter_agent_communication(parent_thread_id, communication)
                .await;
        });
    }
}
