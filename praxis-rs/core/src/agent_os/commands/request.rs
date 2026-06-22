use super::*;

pub(crate) struct AgentOsExecutionOpenRequest<'a> {
    pub(crate) thread_id: ThreadId,
    pub(crate) command: String,
    pub(crate) argv: &'a [String],
    pub(crate) cwd: &'a Path,
    pub(crate) process_id: Option<i32>,
    pub(crate) runtime_kind: Option<&'a str>,
    pub(crate) runtime_owner_id: Option<&'a str>,
}
