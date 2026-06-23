#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::agent_os) enum RuntimeCommandActivity {
    WorkerHeartbeat,
    WorkerStartedCommand,
}
