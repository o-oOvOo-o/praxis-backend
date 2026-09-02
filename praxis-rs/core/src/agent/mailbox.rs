use praxis_protocol::protocol::InterAgentCommunication;
use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tracing::warn;

#[cfg(test)]
use praxis_protocol::AgentPath;

pub(crate) struct Mailbox {
    tx: mpsc::Sender<InterAgentCommunication>,
    next_seq: AtomicU64,
    seq_tx: watch::Sender<u64>,
}

pub(crate) struct MailboxReceiver {
    rx: mpsc::Receiver<InterAgentCommunication>,
    pending_mails: VecDeque<InterAgentCommunication>,
}

impl Mailbox {
    const CAPACITY: usize = 256;

    pub(crate) fn new() -> (Self, MailboxReceiver) {
        let (tx, rx) = mpsc::channel(Self::CAPACITY);
        let (seq_tx, _) = watch::channel(0);
        (
            Self {
                tx,
                next_seq: AtomicU64::new(0),
                seq_tx,
            },
            MailboxReceiver {
                rx,
                pending_mails: VecDeque::new(),
            },
        )
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.seq_tx.subscribe()
    }

    pub(crate) fn send(&self, communication: InterAgentCommunication) -> Option<u64> {
        if let Err(error) = self.tx.try_send(communication) {
            let closed = matches!(error, mpsc::error::TrySendError::Closed(_));
            warn!(
                capacity = Self::CAPACITY,
                closed, "agent mailbox rejected communication"
            );
            return None;
        }
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.seq_tx.send_replace(seq);
        Some(seq)
    }
}

impl MailboxReceiver {
    fn sync_pending_mails(&mut self) {
        while self.pending_mails.len() < Mailbox::CAPACITY
            && let Ok(mail) = self.rx.try_recv()
        {
            self.pending_mails.push_back(mail);
        }
    }

    pub(crate) fn has_pending(&mut self) -> bool {
        self.sync_pending_mails();
        !self.pending_mails.is_empty()
    }

    pub(crate) fn has_pending_trigger_turn(&mut self) -> bool {
        self.sync_pending_mails();
        self.pending_mails.iter().any(|mail| mail.trigger_turn)
    }

    pub(crate) fn take_pending(&mut self) -> VecDeque<InterAgentCommunication> {
        self.sync_pending_mails();
        std::mem::take(&mut self.pending_mails)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn make_mail(
        author: AgentPath,
        recipient: AgentPath,
        content: &str,
        trigger_turn: bool,
    ) -> InterAgentCommunication {
        InterAgentCommunication::new(
            author,
            recipient,
            Vec::new(),
            content.to_string(),
            trigger_turn,
        )
    }

    #[tokio::test]
    async fn mailbox_assigns_monotonic_sequence_numbers() {
        let (mailbox, _receiver) = Mailbox::new();
        let mut seq_rx = mailbox.subscribe();

        let seq_a = mailbox.send(make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "one",
            /*trigger_turn*/ false,
        ));
        let seq_b = mailbox.send(make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "two",
            /*trigger_turn*/ false,
        ));

        seq_rx.changed().await.expect("first seq update");
        assert_eq!(*seq_rx.borrow(), seq_b.expect("second mail accepted"));
        assert_eq!(seq_a, Some(1));
        assert_eq!(seq_b, Some(2));
    }

    #[tokio::test]
    async fn mailbox_drains_in_delivery_order() {
        let (mailbox, mut receiver) = Mailbox::new();
        let mail_one = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "one",
            /*trigger_turn*/ false,
        );
        let mail_two = make_mail(
            AgentPath::try_from("/root/worker").expect("agent path"),
            AgentPath::root(),
            "two",
            /*trigger_turn*/ false,
        );

        mailbox.send(mail_one.clone());
        mailbox.send(mail_two.clone());

        assert_eq!(
            receiver.take_pending(),
            VecDeque::from([mail_one, mail_two])
        );
        assert!(!receiver.has_pending());
    }

    #[tokio::test]
    async fn mailbox_tracks_pending_trigger_turn_mail() {
        let (mailbox, mut receiver) = Mailbox::new();

        mailbox.send(make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "queued",
            /*trigger_turn*/ false,
        ));
        assert!(!receiver.has_pending_trigger_turn());

        mailbox.send(make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "wake",
            /*trigger_turn*/ true,
        ));
        assert!(receiver.has_pending_trigger_turn());
    }

    #[test]
    fn mailbox_rejects_overload_without_growing() {
        let (mailbox, _receiver) = Mailbox::new();
        let worker = AgentPath::try_from("/root/worker").expect("agent path");

        for index in 0..Mailbox::CAPACITY {
            assert_eq!(
                mailbox.send(make_mail(
                    AgentPath::root(),
                    worker.clone(),
                    &index.to_string(),
                    false,
                )),
                Some(index as u64 + 1)
            );
        }
        assert_eq!(
            mailbox.send(make_mail(AgentPath::root(), worker, "overflow", false)),
            None
        );
    }
}
