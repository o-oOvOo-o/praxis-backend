use std::sync::Arc;

use praxis_protocol::items::TurnItem;
use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
use praxis_protocol::protocol::AgentMessageDeltaEvent;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ItemCompletedEvent;
use praxis_protocol::protocol::ReviewOutputEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn process(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    receiver: async_channel::Receiver<Event>,
) -> Option<ReviewOutputEvent> {
    let mut prev_agent_message: Option<Event> = None;
    while let Ok(event) = receiver.recv().await {
        match event.clone().msg {
            EventMsg::AgentMessage(_) => {
                if let Some(prev) = prev_agent_message.take() {
                    session.send_event(ctx.as_ref(), prev.msg).await;
                }
                prev_agent_message = Some(event);
            }
            // Suppress assistant message items because review mode renders structured output.
            EventMsg::ItemCompleted(ItemCompletedEvent {
                item: TurnItem::AgentMessage(_),
                ..
            })
            | EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { .. })
            | EventMsg::AgentMessageContentDelta(AgentMessageContentDeltaEvent { .. }) => {}
            EventMsg::TurnComplete(task_complete) => {
                return task_complete
                    .last_agent_message
                    .as_deref()
                    .map(parse_review_output_event);
            }
            EventMsg::TurnAborted(_) => {
                return None;
            }
            other => {
                session.send_event(ctx.as_ref(), other).await;
            }
        }
    }
    None
}

fn parse_review_output_event(text: &str) -> ReviewOutputEvent {
    if let Ok(ev) = serde_json::from_str::<ReviewOutputEvent>(text) {
        return ev;
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}'))
        && start < end
        && let Some(slice) = text.get(start..=end)
        && let Ok(ev) = serde_json::from_str::<ReviewOutputEvent>(slice)
    {
        return ev;
    }
    ReviewOutputEvent {
        overall_explanation: text.to_string(),
        ..Default::default()
    }
}
