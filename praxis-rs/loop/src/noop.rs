use crate::hooks::TurnHooks;

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopHooks;

impl TurnHooks for NoopHooks {}
