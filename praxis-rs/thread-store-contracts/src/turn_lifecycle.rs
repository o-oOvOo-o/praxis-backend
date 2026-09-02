use crate::TurnAbortReason;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "snake_case")]
pub enum TurnLifecycle {
    Pending,
    Running,
    Completed,
    Aborted(TurnAbortReason),
    Failed { error_code: String },
}

impl TurnLifecycle {
    pub fn apply(
        &mut self,
        transition: TurnTransition,
    ) -> Result<TurnTransitionOutcome, TurnLifecycleError> {
        let target = transition.target();
        if *self == target {
            return Ok(TurnTransitionOutcome::NoOp);
        }
        let allowed = matches!(
            (&*self, &target),
            (Self::Pending, Self::Running)
                | (Self::Running, Self::Completed)
                | (Self::Running, Self::Aborted(_))
                | (Self::Running, Self::Failed { .. })
        );
        if !allowed {
            return Err(TurnLifecycleError {
                from: self.clone(),
                to: target,
            });
        }
        *self = target;
        Ok(TurnTransitionOutcome::Applied)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TurnTransition {
    Start,
    Complete,
    Abort(TurnAbortReason),
    Fail { error_code: String },
}

impl TurnTransition {
    fn target(self) -> TurnLifecycle {
        match self {
            Self::Start => TurnLifecycle::Running,
            Self::Complete => TurnLifecycle::Completed,
            Self::Abort(reason) => TurnLifecycle::Aborted(reason),
            Self::Fail { error_code } => TurnLifecycle::Failed { error_code },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TurnTransitionOutcome {
    Applied,
    NoOp,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid turn lifecycle transition from {from:?} to {to:?}")]
pub struct TurnLifecycleError {
    pub from: TurnLifecycle,
    pub to: TurnLifecycle,
}
