use crate::decisions::ContextPressureDecision;
use crate::decisions::ContextPressureView;
use crate::hooks::TurnHooks;
use crate::model::ModelSpec;
use crate::model::PromptItem;
use crate::outcome::LoopResult;
use crate::services::HistorySink;
use crate::state::TurnState;

pub(crate) async fn apply_context_pressure<S, H>(
    prompt_base: &mut Vec<PromptItem>,
    active_model: &ModelSpec,
    state: &mut TurnState,
    services: &S,
    hooks: &H,
) -> LoopResult<()>
where
    S: HistorySink + ?Sized,
    H: TurnHooks + ?Sized,
{
    match hooks
        .on_context_pressure(ContextPressureView {
            usage: state.token_usage(),
            context_window: active_model.context_window,
        })
        .await
    {
        ContextPressureDecision::Proceed => Ok(()),
        ContextPressureDecision::Compacted {
            prompt_items,
            transcript_items,
        } => {
            *prompt_base = prompt_items;
            if !transcript_items.is_empty() {
                services.persist_items(&transcript_items).await?;
                state.record_items(transcript_items);
            }
            state.mark_transcript_delta_absorbed_by_prompt_refresh();
            Ok(())
        }
        ContextPressureDecision::Abort(reason) => Err(reason),
    }
}
