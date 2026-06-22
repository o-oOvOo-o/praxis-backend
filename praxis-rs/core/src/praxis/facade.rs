use std::sync::Arc;

use async_channel::Receiver;
use async_channel::Sender;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::Submission;
use tokio::sync::watch;

use crate::agent::AgentStatus;

use super::Session;
use super::SessionLoopTermination;

/// The high-level interface to the Praxis system.
/// It operates as a queue pair where callers send submissions and receive events.
pub struct Praxis {
    pub(crate) tx_sub: Sender<Submission>,
    pub(crate) rx_event: Receiver<Event>,
    pub(crate) agent_status: watch::Receiver<AgentStatus>,
    pub(crate) session: Arc<Session>,
    pub(crate) session_loop_termination: SessionLoopTermination,
}
