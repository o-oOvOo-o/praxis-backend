use std::sync::Arc;

use tokio::sync::Mutex;

use super::automation_state;
use super::inbox_runtime;
use super::input::SessionHandleInput;
use super::live_channels;
use super::runtime_state;
use crate::praxis::PermissionLedger;
use crate::praxis::Session;
use crate::state::SessionTokenLedger;

pub(in crate::praxis::session_startup) fn build(input: SessionHandleInput<'_>) -> Arc<Session> {
    let live_channels::SessionLiveChannels {
        out_of_band_elicitation_paused,
    } = live_channels::build();
    let inbox_runtime::SessionInboxRuntime {
        mailbox,
        mailbox_rx,
        idle_pending_input,
    } = inbox_runtime::build();
    let runtime_state::SessionRuntimeState {
        pending_mcp_server_refresh_config,
        conversation,
        active_turn,
        guardian_review_session,
        goal_runtime,
    } = runtime_state::build();
    let automation_state::SessionAutomationState {
        next_internal_sub_id,
        auto_title_attempted,
        auto_summary_in_flight,
    } = automation_state::build();
    let permission_ledger = PermissionLedger::from_session_configuration(
        &input.conversation_id,
        input.session_configuration,
    );
    let token_ledger = SessionTokenLedger::from_state(&input.state);

    Arc::new(Session {
        conversation_id: input.conversation_id,
        tx_event: input.tx_event,
        agent_status: input.agent_status,
        out_of_band_elicitation_paused,
        permission_ledger,
        state: Mutex::new(input.state),
        token_ledger: tokio::sync::RwLock::new(token_ledger),
        features: input.config.features.clone(),
        pending_mcp_server_refresh_config,
        conversation,
        active_turn,
        mailbox,
        mailbox_rx,
        idle_pending_input,
        guardian_review_session,
        services: input.services,
        goal_runtime,
        llm_runtime_catalog: input.llm_runtime_catalog,
        next_internal_sub_id,
        auto_title_attempted,
        auto_summary_in_flight,
    })
}
