use praxis_protocol::ThreadId;

use super::super::ThreadManagerInner;

impl ThreadManagerInner {
    pub(crate) fn notify_thread_created(&self, thread_id: ThreadId) {
        let _ = self.thread_created_tx.send(thread_id);
    }
}
