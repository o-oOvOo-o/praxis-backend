use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseInputItem;

use crate::agent_os::RuntimeCommandRecord;
use crate::praxis::Session;

impl Session {
    pub(in crate::praxis) async fn claim_runtime_command_input_items(
        &self,
    ) -> Vec<ResponseInputItem> {
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
}
