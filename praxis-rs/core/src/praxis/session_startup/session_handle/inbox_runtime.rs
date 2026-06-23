use praxis_protocol::models::ResponseInputItem;
use tokio::sync::Mutex;

use crate::agent::Mailbox;
use crate::agent::MailboxReceiver;

pub(super) struct SessionInboxRuntime {
    pub(super) mailbox: Mailbox,
    pub(super) mailbox_rx: Mutex<MailboxReceiver>,
    pub(super) idle_pending_input: Mutex<Vec<ResponseInputItem>>,
}

pub(super) fn build() -> SessionInboxRuntime {
    let (mailbox, mailbox_rx) = Mailbox::new();
    SessionInboxRuntime {
        mailbox,
        mailbox_rx: Mutex::new(mailbox_rx),
        idle_pending_input: Mutex::new(Vec::new()),
    }
}
