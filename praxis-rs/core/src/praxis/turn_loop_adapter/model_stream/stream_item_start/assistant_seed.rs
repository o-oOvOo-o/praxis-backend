use praxis_protocol::items::AgentMessageContent;
use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseItem;

use crate::turn_assistant_text::raw_assistant_output_text_from_item;

use super::super::assistant_text_stream::AssistantMessageStreamParsers;
use super::super::assistant_text_stream::ParsedAssistantTextDelta;

pub(super) struct StartedAssistantSeed {
    pub(super) item_id: String,
    pub(super) parsed: ParsedAssistantTextDelta,
}

pub(super) fn seed_assistant_text(
    turn_item: &mut TurnItem,
    response_item: &ResponseItem,
    plan_mode: bool,
    parsers: &mut AssistantMessageStreamParsers,
) -> Option<StartedAssistantSeed> {
    if !matches!(turn_item, TurnItem::AgentMessage(_)) {
        return None;
    }

    let raw_text = raw_assistant_output_text_from_item(response_item)?;
    let item_id = turn_item.id();
    let mut seeded = parsers.seed_item_text(&item_id, &raw_text);

    if let TurnItem::AgentMessage(agent_message) = turn_item {
        agent_message.content = vec![AgentMessageContent::Text {
            text: if plan_mode {
                String::new()
            } else {
                std::mem::take(&mut seeded.visible_text)
            },
        }];
    }

    plan_mode.then_some(StartedAssistantSeed {
        item_id,
        parsed: seeded,
    })
}
