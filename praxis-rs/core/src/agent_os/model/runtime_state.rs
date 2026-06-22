use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ThreadRuntimeState {
    Idle,
    Assigned,
    Running,
    WaitingForLease,
    WaitingForCoordinator,
    Stopping,
    Stopped,
    Failed,
    Completed,
}
