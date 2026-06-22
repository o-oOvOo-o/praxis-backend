use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;

use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::Praxis;
use crate::praxis_thread::PraxisThread;

use super::super::ThreadManagerInner;
use super::super::ThreadSpawnResult;

impl ThreadManagerInner {
    pub(super) async fn finalize_thread_spawn(
        &self,
        praxis: Praxis,
        thread_id: ThreadId,
        watch_registration: crate::file_watcher::WatchRegistration,
    ) -> PraxisResult<ThreadSpawnResult> {
        let event = praxis.next_event().await?;
        let session_configured = match event {
            Event {
                id,
                msg: EventMsg::SessionConfigured(session_configured),
            } if id == INITIAL_SUBMIT_ID => session_configured,
            _ => {
                return Err(PraxisErr::SessionConfiguredNotFirstEvent);
            }
        };

        let thread = Arc::new(PraxisThread::new(
            praxis,
            session_configured.rollout_path.clone(),
            watch_registration,
        ));
        self.threads.insert(thread_id, thread.clone()).await;

        Ok(ThreadSpawnResult {
            thread_id,
            thread,
            session_configured,
        })
    }
}
