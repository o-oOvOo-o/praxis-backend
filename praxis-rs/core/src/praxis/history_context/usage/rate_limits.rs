use praxis_protocol::protocol::RateLimitSnapshot;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(crate) async fn update_rate_limits(
        &self,
        turn_context: &TurnContext,
        new_rate_limits: RateLimitSnapshot,
    ) {
        let rate_limits = {
            let mut state = self.state.lock().await;
            state.set_rate_limits(new_rate_limits);
            state.latest_rate_limits.clone()
        };
        self.token_ledger.write().await.set_rate_limits(rate_limits);
        self.send_token_count_event(turn_context).await;
    }
}
