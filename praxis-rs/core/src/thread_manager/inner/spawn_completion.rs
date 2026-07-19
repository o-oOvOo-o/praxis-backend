use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_protocol::config_types::Personality;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::SessionSource;

use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::praxis::INITIAL_SUBMIT_ID;
use crate::praxis::Praxis;
use crate::praxis_thread::{PraxisThread, ThreadConfigSnapshot};

use super::super::ThreadManagerInner;
use super::super::ThreadSpawnResult;

impl ThreadManagerInner {
    pub(super) async fn finalize_thread_spawn(
        &self,
        praxis: Praxis,
        thread_id: ThreadId,
        watch_registration: crate::file_watcher::WatchRegistration,
        has_reserved_thread_id: bool,
        initial_ephemeral: bool,
        initial_personality: Option<Personality>,
        initial_session_source: SessionSource,
    ) -> PraxisResult<ThreadSpawnResult> {
        tracing::info!(%thread_id, "thread spawn awaiting initial session event");
        let event = praxis.next_event().await?;
        tracing::info!(%thread_id, event_id = %event.id, "thread spawn received initial session event");
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
        tracing::info!(%thread_id, "thread spawn registering runtime");
        if !self
            .threads
            .insert(thread_id, thread.clone(), has_reserved_thread_id)
            .await
        {
            return Err(PraxisErr::InvalidRequest(format!(
                "thread `{thread_id}` already exists"
            )));
        }
        tracing::info!(%thread_id, "thread spawn runtime registered");

        let initial_config_snapshot = ThreadConfigSnapshot {
            model: session_configured.model.clone(),
            model_provider_id: session_configured.model_provider_id.clone(),
            service_tier: session_configured.service_tier.clone(),
            approval_policy: session_configured.approval_policy,
            approvals_reviewer: session_configured.approvals_reviewer.clone(),
            sandbox_policy: session_configured.sandbox_policy.clone(),
            cwd: session_configured.cwd.clone(),
            ephemeral: initial_ephemeral,
            reasoning_effort: session_configured.reasoning_effort.clone(),
            personality: initial_personality,
            session_source: initial_session_source,
        };

        Ok(ThreadSpawnResult {
            thread_id,
            thread,
            session_configured,
            initial_config_snapshot,
        })
    }
}
