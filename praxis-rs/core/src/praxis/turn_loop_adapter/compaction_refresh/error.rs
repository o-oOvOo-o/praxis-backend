use praxis_loop::TurnError;
use praxis_loop::TurnErrorKind;

pub(in crate::praxis::turn_loop_adapter) fn internal_turn_error(
    err: impl std::fmt::Display,
) -> TurnError {
    TurnError::new(TurnErrorKind::Internal, err.to_string())
}
