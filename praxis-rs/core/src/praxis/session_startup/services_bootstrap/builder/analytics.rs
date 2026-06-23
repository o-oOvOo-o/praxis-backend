use std::sync::Arc;

use praxis_analytics::AnalyticsEventsClient;
use praxis_login::AuthManager;

use crate::config::Config;

pub(super) fn build(config: &Config, auth_manager: &Arc<AuthManager>) -> AnalyticsEventsClient {
    AnalyticsEventsClient::new(
        Arc::clone(auth_manager),
        config.chatgpt_base_url.trim_end_matches('/').to_string(),
        config.analytics_enabled,
    )
}
