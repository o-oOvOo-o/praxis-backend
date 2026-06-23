use std::sync::Arc;

use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;

pub(super) async fn load(config: Arc<Config>, is_subagent: bool) -> (u64, usize) {
    async {
        if is_subagent {
            (0, 0)
        } else {
            crate::message_history::history_metadata(&config).await
        }
    }
    .instrument(info_span!(
        "session_init.history_metadata",
        otel.name = "session_init.history_metadata",
        session_init.is_subagent = is_subagent,
    ))
    .await
}
