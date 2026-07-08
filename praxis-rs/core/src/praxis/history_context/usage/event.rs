use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::TokenCountEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(in crate::praxis::history_context::usage) async fn send_token_count_event(
        &self,
        turn_context: &TurnContext,
    ) {
        let (info, rate_limits) = self.token_ledger.read().await.token_info_and_rate_limits();
        let event = EventMsg::TokenCount(TokenCountEvent { info, rate_limits });
        self.send_event(turn_context, event).await;
    }
}
