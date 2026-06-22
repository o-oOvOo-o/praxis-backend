use crate::context::TurnContext;
use crate::decisions::PrepareNextRoundDecision;
use crate::decisions::RoundPromptUpdate;
use crate::hooks::TurnHooks;
use crate::model::PromptItem;
use crate::services::RoundSettings;
use crate::state::TurnState;

pub(crate) async fn apply_round_continuation<H>(
    ctx: &TurnContext,
    prompt_base: &mut Vec<PromptItem>,
    state: &mut TurnState,
    active_settings: &mut RoundSettings,
    prompt_update: RoundPromptUpdate,
    hooks: &H,
) where
    H: TurnHooks + ?Sized,
{
    if let RoundPromptUpdate::Replace(prompt_items) = prompt_update {
        *prompt_base = prompt_items;
        state.mark_transcript_delta_absorbed_by_prompt_refresh();
    }
    match hooks.prepare_next_round(ctx).await {
        PrepareNextRoundDecision::Reuse => {}
        PrepareNextRoundDecision::Adjust(adjustment) => {
            if let Some(model) = adjustment.model {
                active_settings.model = model;
            }
            if let Some(reasoning) = adjustment.reasoning {
                active_settings.reasoning = Some(reasoning);
            }
        }
    }
}
