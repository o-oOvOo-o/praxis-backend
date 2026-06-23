use std::sync::Arc;

use async_channel::Receiver;
use praxis_protocol::protocol::Submission;
use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;

use super::super::super::Session;
use super::super::super::main_agent_loop::main_agent_loop;
use super::super::SessionLoopTermination;
use super::super::loop_handle::session_loop_termination_from_handle;

pub(super) fn start(
    session: Arc<Session>,
    config: Arc<Config>,
    rx_sub: Receiver<Submission>,
) -> SessionLoopTermination {
    let thread_id = session.conversation_id;
    let session_loop_handle = tokio::spawn(async move {
        main_agent_loop(session, config, rx_sub)
            .instrument(info_span!("session_loop", thread_id = %thread_id))
            .await;
    });
    session_loop_termination_from_handle(session_loop_handle)
}
