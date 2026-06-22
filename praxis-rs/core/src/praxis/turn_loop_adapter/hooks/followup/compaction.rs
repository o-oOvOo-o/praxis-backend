use praxis_loop::TurnError;

use super::super::super::compaction_decision;
use super::super::super::compaction_refresh::PromptRefreshDecision;
use super::super::PraxisTurnHooks;

#[derive(Clone, Copy)]
pub(in crate::praxis::turn_loop_adapter) enum FollowupCompaction {
    AfterToolRound,
    AfterFinalAnswerPendingInput,
}

pub(super) async fn refresh_followup_prompt(
    hooks: &PraxisTurnHooks,
    compaction: FollowupCompaction,
) -> Result<PromptRefreshDecision, TurnError> {
    match compaction {
        FollowupCompaction::AfterToolRound => {
            compaction_decision::compact_after_tool_round_if_needed(
                &hooks.session,
                &hooks.turn_context,
            )
            .await
        }
        FollowupCompaction::AfterFinalAnswerPendingInput => {
            compaction_decision::compact_before_followup_after_model_round_if_needed(
                &hooks.session,
                &hooks.turn_context,
            )
            .await
        }
    }
}
