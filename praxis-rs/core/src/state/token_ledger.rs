use praxis_protocol::protocol::RateLimitSnapshot;
use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;

use super::session::SessionState;

#[derive(Debug, Clone)]
pub(crate) struct SessionTokenLedger {
    token_info: Option<TokenUsageInfo>,
    rate_limits: Option<RateLimitSnapshot>,
    server_reasoning_included: bool,
}

impl SessionTokenLedger {
    pub(crate) fn from_state(state: &SessionState) -> Self {
        Self {
            token_info: state.token_info(),
            rate_limits: state.latest_rate_limits.clone(),
            server_reasoning_included: state.server_reasoning_included(),
        }
    }

    pub(crate) fn token_info_and_rate_limits(
        &self,
    ) -> (Option<TokenUsageInfo>, Option<RateLimitSnapshot>) {
        (self.token_info.clone(), self.rate_limits.clone())
    }

    pub(crate) fn total_token_usage(&self) -> Option<TokenUsage> {
        self.token_info
            .as_ref()
            .map(|info| info.total_token_usage.clone())
    }

    pub(crate) fn server_reasoning_included(&self) -> bool {
        self.server_reasoning_included
    }

    pub(crate) fn set_token_info(&mut self, token_info: Option<TokenUsageInfo>) {
        self.token_info = token_info;
    }

    pub(crate) fn set_rate_limits(&mut self, rate_limits: Option<RateLimitSnapshot>) {
        self.rate_limits = rate_limits;
    }

    pub(crate) fn set_server_reasoning_included(&mut self, included: bool) {
        self.server_reasoning_included = included;
    }
}
