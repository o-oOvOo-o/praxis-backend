use std::sync::Arc;

use async_channel::Receiver;
use praxis_protocol::protocol::Submission;
use tracing::Instrument;
use tracing::debug;

use crate::config::Config;
use crate::praxis::Session;

mod command_ops;
mod dispatch;
mod dispatch_span;
mod override_turn_context;
mod realtime_ops;
mod response_ops;
mod task_ops;

use dispatch::DispatchOutcome;
use dispatch::dispatch_op;

pub(super) async fn main_agent_loop(
    sess: Arc<Session>,
    config: Arc<Config>,
    rx_sub: Receiver<Submission>,
) {
    // To break out of this loop, send Op::Shutdown.
    while let Ok(sub) = rx_sub.recv().await {
        debug!(?sub, "Submission");
        let dispatch_span = dispatch_span::submission_dispatch_span(&sub);
        let outcome = dispatch_op(&sess, &config, sub.id, sub.op)
            .instrument(dispatch_span)
            .await;
        if outcome == DispatchOutcome::Exit {
            break;
        }
    }
    // Also drain cached guardian state if the submission loop exits because
    // the channel closed without receiving an explicit shutdown op.
    sess.guardian_review_session.shutdown().await;
    debug!("Agent loop exited");
}
