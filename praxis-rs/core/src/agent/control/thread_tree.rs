use std::sync::Arc;

use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

use crate::thread_manager::ThreadManagerInner;

pub(super) async fn resolve_root_thread_id_from_source(
    state: &Arc<ThreadManagerInner>,
    current_thread_id: ThreadId,
    current_session_source: &SessionSource,
    state_db: Option<&Arc<praxis_state::StateRuntime>>,
) -> ThreadId {
    let mut root_thread_id = current_thread_id;
    let mut session_source = current_session_source.clone();
    loop {
        let Some(parent_thread_id) = thread_spawn_parent_thread_id(&session_source) else {
            return root_thread_id;
        };
        root_thread_id = parent_thread_id;
        if let Ok(thread) = state.get_thread(parent_thread_id).await {
            let snapshot = thread.config_snapshot().await;
            session_source = snapshot.session_source;
            continue;
        }
        let Some(state_db) = state_db else {
            return root_thread_id;
        };
        let Ok(Some(metadata)) = state_db.get_thread(parent_thread_id).await else {
            return root_thread_id;
        };
        let Some(parent_source) = parse_session_source_str(metadata.source.as_str()) else {
            return root_thread_id;
        };
        session_source = parent_source;
    }
}

pub(super) async fn is_ancestor_thread_in_source_chain(
    state: &Arc<ThreadManagerInner>,
    ancestor_thread_id: ThreadId,
    current_session_source: &SessionSource,
    state_db: Option<&Arc<praxis_state::StateRuntime>>,
) -> bool {
    let mut session_source = current_session_source.clone();
    loop {
        let Some(parent_thread_id) = thread_spawn_parent_thread_id(&session_source) else {
            return false;
        };
        if parent_thread_id == ancestor_thread_id {
            return true;
        }
        if let Ok(thread) = state.get_thread(parent_thread_id).await {
            let snapshot = thread.config_snapshot().await;
            session_source = snapshot.session_source;
            continue;
        }
        let Some(state_db) = state_db else {
            return false;
        };
        let Ok(Some(metadata)) = state_db.get_thread(parent_thread_id).await else {
            return false;
        };
        let Some(parent_source) = parse_session_source_str(metadata.source.as_str()) else {
            return false;
        };
        session_source = parent_source;
    }
}

fn parse_session_source_str(source: &str) -> Option<SessionSource> {
    serde_json::from_str(source)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(source.to_string())))
        .ok()
}

pub(super) fn thread_spawn_parent_thread_id(session_source: &SessionSource) -> Option<ThreadId> {
    match session_source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        }) => Some(*parent_thread_id),
        _ => None,
    }
}

pub(super) fn agent_matches_prefix(agent_path: Option<&AgentPath>, prefix: &AgentPath) -> bool {
    if prefix.is_root() {
        return true;
    }

    agent_path.is_some_and(|agent_path| {
        agent_path == prefix
            || agent_path
                .as_str()
                .strip_prefix(prefix.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

#[cfg(test)]
pub(super) fn parent_agent_path_from_child_path(
    child_agent_path: Option<&AgentPath>,
) -> Option<AgentPath> {
    child_agent_path
        .and_then(|path| path.as_str().rsplit_once('/').map(|(parent, _)| parent))
        .and_then(|parent| AgentPath::try_from(parent).ok())
}

#[cfg(test)]
pub(super) fn thread_spawn_depth(session_source: &SessionSource) -> Option<i32> {
    match session_source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth, .. }) => Some(*depth),
        _ => None,
    }
}
