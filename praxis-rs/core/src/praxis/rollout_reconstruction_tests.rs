use super::*;

use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::CompactedItem;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::ResumedHistory;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

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

mod active_turn_compaction;
mod previous_turn_settings;
mod reference_context;
mod resumed_rollback;
mod rollback_history;
