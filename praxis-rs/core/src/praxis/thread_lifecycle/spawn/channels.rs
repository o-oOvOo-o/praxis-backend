use async_channel::Receiver;
use async_channel::Sender;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::Submission;
use tokio::sync::watch;

use crate::agent::AgentStatus;

use super::super::SUBMISSION_CHANNEL_CAPACITY;

pub(super) struct SpawnChannels {
    pub(super) tx_sub: Sender<Submission>,
    pub(super) rx_sub: Receiver<Submission>,
    pub(super) tx_event: Sender<Event>,
    pub(super) rx_event: Receiver<Event>,
    pub(super) agent_status_tx: watch::Sender<AgentStatus>,
    pub(super) agent_status_rx: watch::Receiver<AgentStatus>,
}

pub(super) fn open() -> SpawnChannels {
    let (tx_sub, rx_sub) = async_channel::bounded(SUBMISSION_CHANNEL_CAPACITY);
    let (tx_event, rx_event) = async_channel::unbounded();
    let (agent_status_tx, agent_status_rx) = watch::channel(AgentStatus::PendingInit);
    SpawnChannels {
        tx_sub,
        rx_sub,
        tx_event,
        rx_event,
        agent_status_tx,
        agent_status_rx,
    }
}
