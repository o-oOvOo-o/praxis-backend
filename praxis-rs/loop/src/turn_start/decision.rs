use crate::context::TurnContext;
use crate::decisions::TurnStartDecision;
use crate::hooks::TurnHooks;
use crate::outcome::TurnError;

pub(super) async fn apply_turn_start_decision<H>(
    ctx: &mut TurnContext,
    hooks: &H,
) -> Result<(), TurnError>
where
    H: TurnHooks + ?Sized,
{
    match hooks.on_turn_start(ctx).await {
        TurnStartDecision::Proceed => Ok(()),
        TurnStartDecision::ReplaceInitialPrompt(prompt_items) => {
            ctx.initial_prompt_items = prompt_items;
            Ok(())
        }
        TurnStartDecision::Abort(reason) => Err(reason),
    }
}
