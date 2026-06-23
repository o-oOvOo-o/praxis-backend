use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_rollout::RolloutRecorderParams;
use praxis_state::ThreadMetadataBuilder;

use crate::praxis::SessionConfiguration;
use crate::rollout::policy::EventPersistenceMode;

mod recorder_params;
mod state_metadata;

pub(super) struct RolloutBootstrap {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) params: RolloutRecorderParams,
    pub(super) state_builder: Option<ThreadMetadataBuilder>,
}

pub(super) fn build(
    initial_history: &InitialHistory,
    session_configuration: &SessionConfiguration,
    session_source: SessionSource,
) -> RolloutBootstrap {
    let forked_from_id = initial_history.forked_from_id();
    let persistence_mode = if session_configuration.persist_extended_history {
        EventPersistenceMode::Extended
    } else {
        EventPersistenceMode::Limited
    };

    let recorder_params::RecorderParamsBootstrap {
        conversation_id,
        params,
    } = recorder_params::build(recorder_params::RecorderParamsInput {
        initial_history,
        session_configuration,
        session_source,
        forked_from_id,
        persistence_mode,
    });
    let state_builder = state_metadata::builder_from_initial_history(initial_history);

    RolloutBootstrap {
        conversation_id,
        forked_from_id,
        params,
        state_builder,
    }
}
