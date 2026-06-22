use crate::decisions::SteeringDecision;
use crate::decisions::SteeringInputView;
use crate::hooks::TurnHooks;
use crate::model::PromptItem;
use crate::outcome::LoopResult;
use crate::outcome::TurnCompletionMessage;
use crate::prompt::build_round_prompt;
use crate::services::SteeringControl;
use crate::services::SteeringInbox;
use crate::state::TurnState;

pub(crate) enum RoundPromptDecision {
    Sample(Vec<PromptItem>),
    RetryWithoutModelRequest,
    StopWithoutModelRequest(TurnCompletionMessage),
}

pub(crate) async fn prepare_round_prompt<S, H>(
    prompt_base: &[PromptItem],
    state: &TurnState,
    services: &S,
    hooks: &H,
) -> LoopResult<RoundPromptDecision>
where
    S: SteeringInbox + ?Sized,
    H: TurnHooks + ?Sized,
{
    let mut prompt = build_round_prompt(prompt_base, state);
    let drain = services.drain_steering().await?;
    match drain.control {
        SteeringControl::RetryWithoutModelRequest => {
            return Ok(RoundPromptDecision::RetryWithoutModelRequest);
        }
        SteeringControl::StopWithoutModelRequest(message) => {
            return Ok(RoundPromptDecision::StopWithoutModelRequest(message));
        }
        SteeringControl::Continue => {}
    }

    if !drain.messages.is_empty() {
        match hooks
            .on_steering_input(SteeringInputView {
                messages: &drain.messages,
            })
            .await
        {
            SteeringDecision::InjectAndContinue => {
                for message in drain.messages {
                    prompt.extend(message.prompt_items);
                }
            }
            SteeringDecision::DropAndContinue => {}
        }
    }

    Ok(RoundPromptDecision::Sample(prompt))
}
