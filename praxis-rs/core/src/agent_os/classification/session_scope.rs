use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

use crate::agent_os::policy::COORDINATOR_RANK;

pub(crate) fn rank_for_session_source(source: &SessionSource) -> u8 {
    match source {
        SessionSource::SubAgent(_) => 2,
        _ => COORDINATOR_RANK,
    }
}

pub(crate) fn profile_for_rank(rank: u8) -> &'static str {
    match rank {
        COORDINATOR_RANK => "coordinator",
        _ => "worker",
    }
}

pub(crate) fn coordination_scope_for_session_source(
    source: &SessionSource,
    thread_id: ThreadId,
) -> String {
    match source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        }) => format!("root:{parent_thread_id}"),
        _ => format!("root:{thread_id}"),
    }
}
