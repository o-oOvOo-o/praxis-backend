use crate::outcome::RoundOutcome;
use crate::outcome::TurnCompletionMessage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FollowupSignal {
    None,
    Required,
}

impl Default for FollowupSignal {
    fn default() -> Self {
        Self::None
    }
}

impl FollowupSignal {
    pub(super) fn require(&mut self) {
        *self = Self::Required;
    }

    pub(super) fn into_round_outcome(self, final_text: Option<String>) -> RoundOutcome {
        match self {
            Self::Required => RoundOutcome::FollowupRequired,
            Self::None => match final_text {
                Some(message) => RoundOutcome::FinalAnswer {
                    message: TurnCompletionMessage::text(message),
                },
                None => RoundOutcome::Empty,
            },
        }
    }
}
