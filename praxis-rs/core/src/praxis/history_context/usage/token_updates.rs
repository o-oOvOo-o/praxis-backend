use praxis_protocol::protocol::TokenUsage;
use praxis_protocol::protocol::TokenUsageInfo;

use crate::error::ContextOverflowError;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::praxis::turn_compaction::effective_auto_compact_token_limit;

impl Session {
    pub(crate) async fn update_token_usage_info(
        &self,
        turn_context: &TurnContext,
        token_usage: Option<&TokenUsage>,
    ) {
        if let Some(token_usage) = token_usage {
            let effective_context_window = self
                .context_governance
                .effective_context_window(turn_context)
                .await;
            let auto_compact_limit = effective_auto_compact_token_limit(self, turn_context).await;
            let token_info = {
                let mut state = self.state.lock().await;
                state.update_token_info_from_usage(
                    token_usage,
                    effective_context_window,
                    auto_compact_limit,
                );
                state.token_info()
            };
            self.token_ledger.write().await.set_token_info(token_info);
        }
        self.send_token_count_event(turn_context).await;
    }

    pub(crate) async fn recompute_token_usage(&self, turn_context: &TurnContext) {
        let history = self.clone_history().await;
        let base_instructions = self.get_base_instructions().await;
        let Some(estimated_total_tokens) =
            history.estimate_token_count_with_base_instructions(&base_instructions)
        else {
            return;
        };
        let effective_context_window = self
            .context_governance
            .effective_context_window(turn_context)
            .await;
        let auto_compact_limit = effective_auto_compact_token_limit(self, turn_context).await;
        let token_info = {
            let mut state = self.state.lock().await;
            let mut info = state.token_info().unwrap_or(TokenUsageInfo {
                total_token_usage: TokenUsage::default(),
                last_token_usage: TokenUsage::default(),
                internal_savings: Default::default(),
                model_context_window: None,
                model_auto_compact_token_limit: None,
            });

            info.last_token_usage = TokenUsage {
                input_tokens: 0,
                cached_input_tokens: 0,
                cache_reported_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: estimated_total_tokens.max(0),
            };

            if let Some(model_context_window) = effective_context_window {
                info.model_context_window = Some(model_context_window);
            }
            if let Some(model_auto_compact_token_limit) = auto_compact_limit {
                info.model_auto_compact_token_limit = Some(model_auto_compact_token_limit);
            }

            state.set_token_info(Some(info));
            state.token_info()
        };
        self.token_ledger.write().await.set_token_info(token_info);
        self.send_token_count_event(turn_context).await;
    }

    pub(crate) async fn record_token_saving_event(
        &self,
        turn_context: &TurnContext,
        event: praxis_protocol::protocol::TokenSavingEvent,
    ) {
        if !event.reversible || event.saved_tokens() <= 0 {
            return;
        }
        let token_info = {
            let mut state = self.state.lock().await;
            state.record_token_saving_event(event);
            state.token_info()
        };
        self.token_ledger.write().await.set_token_info(token_info);
        self.send_token_count_event(turn_context).await;
    }

    pub(crate) async fn set_total_tokens_full(&self, turn_context: &TurnContext) {
        let effective_context_window = self
            .context_governance
            .effective_context_window(turn_context)
            .await;
        let token_info = if let Some(context_window) = effective_context_window {
            let mut state = self.state.lock().await;
            state.set_token_usage_full(context_window);
            state.token_info()
        } else {
            self.token_ledger
                .read()
                .await
                .token_info_and_rate_limits()
                .0
        };
        self.token_ledger.write().await.set_token_info(token_info);
        self.send_token_count_event(turn_context).await;
    }

    pub(crate) async fn observe_context_overflow(
        &self,
        turn_context: &TurnContext,
        overflow: &ContextOverflowError,
    ) {
        if self
            .context_governance
            .observe_overflow(turn_context, overflow)
            .await
            .is_some()
        {
            self.set_total_tokens_full(turn_context).await;
        }
    }
}
