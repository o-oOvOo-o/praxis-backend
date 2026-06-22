use praxis_loop::decisions::RoundDecision;
use praxis_loop::decisions::RoundOutcomeView;
use praxis_loop::outcome::RoundOutcome;
use praxis_loop::outcome::TurnCompletionMessage;

use super::PraxisTurnHooks;
use super::followup;
use super::followup::FollowupCompaction;

pub(super) async fn after_model_round(
    hooks: &PraxisTurnHooks,
    view: RoundOutcomeView<'_>,
) -> RoundDecision {
    match view.outcome {
        RoundOutcome::FollowupRequired | RoundOutcome::ToolCalls { .. } => {
            followup::continue_followup_round(hooks, FollowupCompaction::AfterToolRound).await
        }
        RoundOutcome::FinalAnswer { message } => {
            if hooks
                .session
                .has_pending_input_bounded("model_round_completed")
                .await
            {
                return followup::continue_followup_round(
                    hooks,
                    FollowupCompaction::AfterFinalAnswerPendingInput,
                )
                .await;
            }
            hooks.bridge_state.record_completion_message(message).await;
            RoundDecision::Stop(message.clone())
        }
        RoundOutcome::TerminatedByTool { message } => {
            hooks.bridge_state.record_completion_message(message).await;
            RoundDecision::Stop(message.clone())
        }
        RoundOutcome::Empty => RoundDecision::Stop(TurnCompletionMessage::NoMessage),
    }
}
