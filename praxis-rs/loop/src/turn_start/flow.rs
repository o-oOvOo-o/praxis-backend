use crate::context::TurnContext;
use crate::model::PromptItem;
use crate::outcome::TurnResult;
use crate::services::RoundSettings;
use crate::state::TurnState;

pub(crate) enum TurnStartFlow {
    Ready(TurnStartReady),
    Finished(TurnResult),
}

pub(crate) struct TurnStartReady {
    ctx: TurnContext,
    state: TurnState,
    prompt_base: Vec<PromptItem>,
    active_settings: RoundSettings,
}

impl TurnStartReady {
    pub(super) fn new(ctx: TurnContext, state: TurnState, prompt_base: Vec<PromptItem>) -> Self {
        let active_settings = RoundSettings {
            model: ctx.model.clone(),
            reasoning: ctx.reasoning.clone(),
            service_tier: ctx.service_tier.clone(),
        };
        Self {
            ctx,
            state,
            prompt_base,
            active_settings,
        }
    }

    pub(crate) fn into_parts(self) -> (TurnContext, TurnState, Vec<PromptItem>, RoundSettings) {
        (self.ctx, self.state, self.prompt_base, self.active_settings)
    }
}
