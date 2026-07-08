use praxis_protocol::protocol::TokenUsage;

use crate::context_manager::TotalTokenUsageBreakdown;

use super::super::Session;

impl Session {
    pub(crate) async fn get_total_token_usage(&self) -> i64 {
        let server_reasoning_included = self.token_ledger.read().await.server_reasoning_included();
        let state = self.state.lock().await;
        state.get_total_token_usage(server_reasoning_included)
    }

    pub(crate) async fn get_total_token_usage_breakdown(&self) -> TotalTokenUsageBreakdown {
        let state = self.state.lock().await;
        state.history.get_total_token_usage_breakdown()
    }

    pub(crate) async fn total_token_usage(&self) -> Option<TokenUsage> {
        let token_ledger = self.token_ledger.read().await;
        token_ledger.total_token_usage()
    }
}
