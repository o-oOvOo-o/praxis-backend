use super::assistant_text_stream::AssistantMessageStreamParsers;
use super::plan_mode_stream::PlanModeStreamState;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::items::TurnItem;

use crate::praxis::TurnContext;

mod completion;
mod delta;
mod flush;
mod reasoning;
mod start;

pub(super) struct StreamItemState {
    active_item: Option<TurnItem>,
    last_agent_message: Option<String>,
    plan_mode: bool,
    assistant_message_stream_parsers: AssistantMessageStreamParsers,
    plan_mode_state: Option<PlanModeStreamState>,
}

impl StreamItemState {
    pub(super) fn new(turn_context: &TurnContext) -> Self {
        let plan_mode = turn_context.collaboration_mode.mode == ModeKind::Plan;
        Self {
            active_item: None,
            last_agent_message: None,
            plan_mode,
            assistant_message_stream_parsers: AssistantMessageStreamParsers::new(plan_mode),
            plan_mode_state: plan_mode.then(|| PlanModeStreamState::new(&turn_context.sub_id)),
        }
    }

    pub(super) fn active_item_id(&self) -> Option<String> {
        self.active_item.as_ref().map(TurnItem::id)
    }
}
