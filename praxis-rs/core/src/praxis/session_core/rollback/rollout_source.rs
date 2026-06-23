use std::path::Path;
use std::path::PathBuf;

use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::PraxisErrorInfo;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::rollout::RolloutRecorder;

pub(super) async fn load_flushed_history(
    session: &Session,
    turn_context: &TurnContext,
) -> Option<InitialHistory> {
    let rollout_path = persisted_rollout_path(session, &turn_context.sub_id).await?;
    if !flush_current_rollout(session, &turn_context.sub_id, &rollout_path).await {
        return None;
    }
    load_rollout_history(session, &turn_context.sub_id, &rollout_path).await
}

async fn persisted_rollout_path(session: &Session, event_id: &str) -> Option<PathBuf> {
    let recorder = {
        let guard = session.services.rollout.lock().await;
        guard.clone()
    };
    let Some(recorder) = recorder else {
        session
            .raw_event_emitter(event_id)
            .error(
                "thread rollback requires a persisted rollout path",
                Some(PraxisErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return None;
    };
    Some(recorder.rollout_path().to_path_buf())
}

async fn flush_current_rollout(session: &Session, event_id: &str, rollout_path: &Path) -> bool {
    let recorder = {
        let guard = session.services.rollout.lock().await;
        guard.clone()
    };
    if let Some(recorder) = recorder
        && let Err(err) = recorder.flush().await
    {
        session
            .raw_event_emitter(event_id)
            .error(
                format!(
                    "failed to flush rollout `{}` for rollback replay: {err}",
                    rollout_path.display()
                ),
                Some(PraxisErrorInfo::ThreadRollbackFailed),
            )
            .await;
        return false;
    }
    true
}

async fn load_rollout_history(
    session: &Session,
    event_id: &str,
    rollout_path: &Path,
) -> Option<InitialHistory> {
    match RolloutRecorder::get_rollout_history(rollout_path).await {
        Ok(history) => Some(history),
        Err(err) => {
            session
                .raw_event_emitter(event_id)
                .error(
                    format!(
                        "failed to load rollout `{}` for rollback replay: {err}",
                        rollout_path.display()
                    ),
                    Some(PraxisErrorInfo::ThreadRollbackFailed),
                )
                .await;
            None
        }
    }
}
