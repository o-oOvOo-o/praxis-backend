use praxis_protocol::ThreadId;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_rollout::RolloutRecorderParams;
use praxis_state::ThreadMetadataBuilder;

use crate::praxis::SessionConfiguration;
use crate::rollout::metadata;
use crate::rollout::policy::EventPersistenceMode;

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

    let (conversation_id, params) = match initial_history {
        InitialHistory::New | InitialHistory::Forked(_) => {
            let conversation_id = ThreadId::default();
            (
                conversation_id,
                RolloutRecorderParams::new(
                    conversation_id,
                    forked_from_id,
                    session_source,
                    BaseInstructions {
                        text: session_configuration.base_instructions.clone(),
                    },
                    session_configuration.dynamic_tools.clone(),
                    persistence_mode,
                ),
            )
        }
        InitialHistory::Resumed(resumed_history) => (
            resumed_history.conversation_id,
            RolloutRecorderParams::resume(resumed_history.rollout_path.clone(), persistence_mode),
        ),
    };

    let state_builder = match initial_history {
        InitialHistory::Resumed(resumed) => {
            metadata::builder_from_items(resumed.history.as_slice(), resumed.rollout_path.as_path())
        }
        InitialHistory::New | InitialHistory::Forked(_) => None,
    };

    RolloutBootstrap {
        conversation_id,
        forked_from_id,
        params,
        state_builder,
    }
}
