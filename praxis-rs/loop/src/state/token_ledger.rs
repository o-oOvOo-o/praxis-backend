use serde::Deserialize;
use serde::Serialize;

use crate::model::TokenUsage;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenLedger {
    pub cumulative: TokenUsage,
    pub turn_delta: TokenUsage,
}

impl TokenLedger {
    pub fn record_usage(&mut self, usage: &TokenUsage) {
        self.cumulative.add_assign(usage);
        self.turn_delta.add_assign(usage);
    }

    pub fn pressure_ratio(&self, context_window: Option<u64>) -> Option<f64> {
        let window = context_window?;
        if window == 0 {
            return None;
        }
        Some(self.cumulative.total as f64 / window as f64)
    }
}
