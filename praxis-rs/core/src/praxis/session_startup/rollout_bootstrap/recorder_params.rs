use praxis_protocol::ThreadId;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_rollout::RolloutRecorderParams;

use crate::praxis::SessionConfiguration;
use crate::rollout::policy::EventPersistenceMode;

pub(super) struct RecorderParamsInput<'a> {
    pub(super) initial_history: &'a InitialHistory,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) session_source: SessionSource,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) persistence_mode: EventPersistenceMode,
}

pub(super) struct RecorderParamsBootstrap {
    pub(super) conversation_id: ThreadId,
    pub(super) params: RolloutRecorderParams,
}

pub(super) fn build(input: RecorderParamsInput<'_>) -> RecorderParamsBootstrap {
    match input.initial_history {
        InitialHistory::New | InitialHistory::Forked(_) => new_thread_params(input),
        InitialHistory::Resumed(resumed_history) => RecorderParamsBootstrap {
            conversation_id: resumed_history.conversation_id,
            params: RolloutRecorderParams::resume(
                resumed_history.rollout_path.clone(),
                input.persistence_mode,
            ),
        },
    }
}

fn new_thread_params(input: RecorderParamsInput<'_>) -> RecorderParamsBootstrap {
    let conversation_id = ThreadId::default();
    RecorderParamsBootstrap {
        conversation_id,
        params: RolloutRecorderParams::new(
            conversation_id,
            input.forked_from_id,
            input.session_source,
            BaseInstructions {
                text: input.session_configuration.base_instructions.clone(),
            },
            input.session_configuration.dynamic_tools.clone(),
            input.persistence_mode,
        ),
    }
}
