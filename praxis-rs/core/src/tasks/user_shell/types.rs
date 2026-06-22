pub(super) const USER_SHELL_TIMEOUT_MS: u64 = 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UserShellCommandMode {
    StandaloneTurn,
    ActiveTurnAuxiliary,
}
