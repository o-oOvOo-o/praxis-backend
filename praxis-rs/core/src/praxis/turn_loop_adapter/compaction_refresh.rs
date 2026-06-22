mod error;
mod policy;
mod prompt_refresh;

use praxis_loop::decisions::RoundPromptUpdate;

pub(super) use error::internal_turn_error;
pub(super) use policy::auto_compact_needed;
pub(super) use prompt_refresh::auto_compact_prompt_refresh;
pub(super) use prompt_refresh::auto_compact_prompt_refresh_if_needed;
pub(super) use prompt_refresh::prompt_items_from_session_history;

pub(super) type LoopPromptItems = Vec<praxis_loop::model::PromptItem>;

pub(super) enum PromptRefreshDecision {
    Unchanged,
    Refreshed(LoopPromptItems),
}

impl PromptRefreshDecision {
    pub(super) fn into_round_prompt_update(self) -> RoundPromptUpdate {
        match self {
            Self::Unchanged => RoundPromptUpdate::Reuse,
            Self::Refreshed(prompt_items) => RoundPromptUpdate::Replace(prompt_items),
        }
    }
}
