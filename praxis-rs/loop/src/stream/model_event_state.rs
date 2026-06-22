use crate::model::ModelEvent;
use crate::model::TurnEvent;
use crate::model::TurnItem;
use crate::outcome::LoopResult;
use crate::outcome::RoundOutcome;
use crate::services::EventSink;
use crate::state::TurnState;
use crate::tool::ToolCall;

use super::followup::FollowupSignal;
use super::text_accumulator::AssistantTextAccumulator;

#[derive(Default)]
pub(super) struct ModelStreamState {
    calls: Vec<ToolCall>,
    new_items: Vec<TurnItem>,
    assistant_text: AssistantTextAccumulator,
    followup: FollowupSignal,
}

pub(super) struct ModelStreamCompletion {
    pub(super) calls: Vec<ToolCall>,
    pub(super) new_items: Vec<TurnItem>,
    pub(super) no_tool_outcome: RoundOutcome,
}

impl ModelStreamState {
    pub(super) async fn record_event<S>(
        &mut self,
        event: ModelEvent,
        state: &mut TurnState,
        services: &S,
    ) -> LoopResult<bool>
    where
        S: EventSink + ?Sized,
    {
        match event {
            ModelEvent::TextDelta { item_id, text } => {
                self.assistant_text.push_delta(&item_id, &text);
                services
                    .emit_event(TurnEvent::TextDelta { item_id, text })
                    .await?;
            }
            ModelEvent::ReasoningDelta {
                item_id,
                summary_index,
                content_index,
                text,
            } => {
                services
                    .emit_event(TurnEvent::ReasoningDelta {
                        item_id: item_id.clone(),
                        summary_index,
                        content_index,
                        text: text.clone(),
                    })
                    .await?;
                self.new_items.push(TurnItem::Reasoning { item_id, text });
            }
            ModelEvent::ToolCall(call) => {
                self.calls.push(call);
            }
            ModelEvent::FinalText { item_id, text } => {
                self.assistant_text.push_final(item_id, text);
            }
            ModelEvent::RecordedFinalText { item_id, text } => {
                self.assistant_text.push_recorded_final(item_id, text);
            }
            ModelEvent::FollowupRequired => {
                self.followup.require();
            }
            ModelEvent::Completed(usage) => {
                state.record_usage(&usage);
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub(super) fn complete(mut self, state: &mut TurnState) -> ModelStreamCompletion {
        let final_text = self.assistant_text.commit(state, &mut self.new_items);
        ModelStreamCompletion {
            calls: self.calls,
            new_items: self.new_items,
            no_tool_outcome: self.followup.into_round_outcome(final_text),
        }
    }
}
