use std::collections::HashMap;
use std::collections::VecDeque;

use tokio::sync::RwLock;

use crate::error::ContextOverflowError;

use super::TurnContext;

const DEFAULT_TRIGGER_PERCENT: i64 = 85;
const DEFAULT_RESERVED_TOKENS: i64 = 50_000;
const MAX_RESERVED_WINDOW_PERCENT: i64 = 20;
const OVERFLOW_SAFETY_PERCENT: i64 = 85;
const MAX_OBSERVED_CONTEXT_LIMITS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RuntimeModelKey {
    provider_id: String,
    endpoint: Option<String>,
    model: String,
}

#[derive(Debug, Clone, Copy)]
struct ObservedContextLimit {
    tokens: i64,
}

#[derive(Debug, Default)]
pub(crate) struct ContextGovernanceState {
    observed_limits: RwLock<ObservedContextLimits>,
}

#[derive(Debug, Default)]
struct ObservedContextLimits {
    by_runtime: HashMap<RuntimeModelKey, ObservedContextLimit>,
    insertion_order: VecDeque<RuntimeModelKey>,
}

impl ContextGovernanceState {
    pub(crate) async fn observe_overflow(
        &self,
        turn_context: &TurnContext,
        overflow: &ContextOverflowError,
    ) -> Option<i64> {
        let observed = overflow
            .context_limit
            .filter(|limit| *limit > 0)
            .or_else(|| {
                overflow
                    .requested_tokens
                    .filter(|tokens| *tokens > 0)
                    .map(|tokens| tokens.saturating_mul(OVERFLOW_SAFETY_PERCENT) / 100)
            })?;
        let key = RuntimeModelKey::from_turn_context(turn_context);
        let mut limits = self.observed_limits.write().await;
        if !limits.by_runtime.contains_key(&key) {
            limits.insertion_order.push_back(key.clone());
        }
        let entry = limits
            .by_runtime
            .entry(key)
            .or_insert(ObservedContextLimit { tokens: observed });
        entry.tokens = entry.tokens.min(observed);
        let effective = entry.tokens;
        while limits.by_runtime.len() > MAX_OBSERVED_CONTEXT_LIMITS {
            if let Some(expired) = limits.insertion_order.pop_front() {
                limits.by_runtime.remove(&expired);
            }
        }
        Some(effective)
    }

    pub(crate) async fn effective_context_window(&self, turn_context: &TurnContext) -> Option<i64> {
        let catalog = turn_context.model_context_window();
        let observed = self
            .observed_limits
            .read()
            .await
            .by_runtime
            .get(&RuntimeModelKey::from_turn_context(turn_context))
            .map(|limit| limit.tokens);
        match (catalog, observed) {
            (Some(catalog), Some(observed)) => Some(catalog.min(observed)),
            (Some(catalog), None) => Some(catalog),
            (None, Some(observed)) => Some(observed),
            (None, None) => None,
        }
    }

    pub(crate) async fn compact_threshold(&self, turn_context: &TurnContext) -> Option<i64> {
        let window = self.effective_context_window(turn_context).await?;
        let ratio_limit = window.saturating_mul(DEFAULT_TRIGGER_PERCENT) / 100;
        let reserved =
            DEFAULT_RESERVED_TOKENS.min(window.saturating_mul(MAX_RESERVED_WINDOW_PERCENT) / 100);
        Some(ratio_limit.min(window.saturating_sub(reserved)).max(1))
    }
}

impl RuntimeModelKey {
    fn from_turn_context(turn_context: &TurnContext) -> Self {
        Self {
            provider_id: turn_context.config.model_provider_id.clone(),
            endpoint: turn_context.provider.base_url.clone(),
            model: turn_context.model_info.slug.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_budget_scales_down_for_small_windows() {
        let window = 100_000_i64;
        let ratio_limit = window * DEFAULT_TRIGGER_PERCENT / 100;
        let reserved = DEFAULT_RESERVED_TOKENS.min(window * MAX_RESERVED_WINDOW_PERCENT / 100);
        assert_eq!(ratio_limit.min(window - reserved), 80_000);
    }
}
