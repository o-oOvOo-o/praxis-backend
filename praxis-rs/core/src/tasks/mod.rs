mod abort;
mod cleanup;
mod compact;
mod finish;
mod ghost_snapshot;
mod metrics;
mod pending_work;
mod regular;
mod review;
mod spawn;
mod task_trait;
mod undo;
mod user_shell;

pub(crate) use abort::interrupted_turn_history_marker;
#[cfg(test)]
pub(crate) use regular::RegularAgentTask;
#[cfg(test)]
pub(crate) use review::ReviewTask;
pub(crate) use task_trait::AgentTask;
#[cfg(test)]
pub(crate) use task_trait::AgentTaskContext;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
