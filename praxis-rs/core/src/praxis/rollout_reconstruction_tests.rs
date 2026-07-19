pub(super) use super::*;

pub(super) use praxis_protocol::AgentPath;
pub(super) use praxis_protocol::ThreadId;
pub(super) use praxis_protocol::models::ContentItem;
pub(super) use praxis_protocol::models::ResponseItem;
pub(super) use praxis_protocol::protocol::CompactedItem;
pub(super) use praxis_protocol::protocol::InitialHistory;
pub(super) use praxis_protocol::protocol::InterAgentCommunication;
pub(super) use praxis_protocol::protocol::ResumedHistory;
pub(super) use std::path::PathBuf;

fn user_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn assistant_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn inter_agent_assistant_message(text: &str) -> ResponseItem {
    let communication = InterAgentCommunication::new(
        AgentPath::root(),
        AgentPath::root().join("worker").unwrap(),
        Vec::new(),
        text.to_string(),
        /*trigger_turn*/ true,
    );
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: serde_json::to_string(&communication).unwrap(),
        }],
        end_turn: None,
        phase: None,
    }
}

#[path = "rollout_reconstruction_tests/active_turn_compaction.rs"]
mod active_turn_compaction;
#[path = "rollout_reconstruction_tests/previous_turn_settings.rs"]
mod previous_turn_settings;
#[path = "rollout_reconstruction_tests/reference_context.rs"]
mod reference_context;
#[path = "rollout_reconstruction_tests/resumed_rollback.rs"]
mod resumed_rollback;
#[path = "rollout_reconstruction_tests/rollback_history.rs"]
mod rollback_history;
