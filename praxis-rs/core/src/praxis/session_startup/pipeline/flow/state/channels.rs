use async_channel::Sender;
use praxis_protocol::protocol::Event;
use tokio::sync::watch;

use crate::agent::AgentStatus;

pub(in crate::praxis::session_startup::pipeline::flow) struct SessionStartupChannels {
    pub(in crate::praxis::session_startup::pipeline::flow) tx_event: Sender<Event>,
    pub(in crate::praxis::session_startup::pipeline::flow) agent_status: watch::Sender<AgentStatus>,
}
