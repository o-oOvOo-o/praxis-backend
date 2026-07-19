use crate::num_format::format_with_separators;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq, JsonSchema, TS)]
pub struct TokenUsage {
    #[serde(default)]
    #[ts(type = "number")]
    pub input_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub cached_input_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub cache_reported_input_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub output_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub reasoning_output_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct TokenUsageInfo {
    pub total_token_usage: TokenUsage,
    pub last_token_usage: TokenUsage,
    #[serde(default)]
    pub internal_savings: TokenSavingsInfo,
    // TODO(aibrahim): make this not optional
    #[ts(type = "number | null")]
    pub model_context_window: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub model_auto_compact_token_limit: Option<i64>,
}

impl TokenUsageInfo {
    pub fn new_or_append(
        info: &Option<TokenUsageInfo>,
        last: &Option<TokenUsage>,
        model_context_window: Option<i64>,
        model_auto_compact_token_limit: Option<i64>,
    ) -> Option<Self> {
        if info.is_none() && last.is_none() {
            return None;
        }

        let mut info = match info {
            Some(info) => info.clone(),
            None => Self {
                total_token_usage: TokenUsage::default(),
                last_token_usage: TokenUsage::default(),
                internal_savings: TokenSavingsInfo::default(),
                model_context_window,
                model_auto_compact_token_limit,
            },
        };
        if let Some(last) = last {
            info.append_last_usage(last);
        }
        if let Some(model_context_window) = model_context_window {
            info.model_context_window = Some(model_context_window);
        }
        if let Some(model_auto_compact_token_limit) = model_auto_compact_token_limit {
            info.model_auto_compact_token_limit = Some(model_auto_compact_token_limit);
        }
        Some(info)
    }

    pub fn append_last_usage(&mut self, last: &TokenUsage) {
        self.total_token_usage.add_assign(last);
        self.last_token_usage = last.clone();
    }

    pub fn fill_to_context_window(&mut self, context_window: i64) {
        // Mark the CONTEXT as full so the auto-compact trigger (which reads
        // `last_token_usage`) fires, but leave `total_token_usage` untouched:
        // it is the cumulative ledger of real spend, and overwriting it to the
        // window size on a (possibly provider-misreported) context-window
        // error permanently corrupts goal token budgets and persisted usage.
        self.model_context_window = Some(context_window);
        self.last_token_usage = TokenUsage {
            input_tokens: context_window,
            total_tokens: context_window,
            ..TokenUsage::default()
        };
    }

    pub fn full_context_window(context_window: i64) -> Self {
        let mut info = Self {
            total_token_usage: TokenUsage::default(),
            last_token_usage: TokenUsage::default(),
            internal_savings: TokenSavingsInfo::default(),
            model_context_window: Some(context_window),
            model_auto_compact_token_limit: None,
        };
        info.fill_to_context_window(context_window);
        info
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq, JsonSchema, TS)]
pub struct TokenSavingsInfo {
    #[serde(default)]
    #[ts(type = "number")]
    pub total_saved_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub last_saved_tokens: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<TokenSavingCategoryUsage>,
}

impl TokenSavingsInfo {
    pub fn record(&mut self, saved_tokens: i64) {
        self.record_event(TokenSavingEvent::reversible(
            TokenSavingKind::ToolOutputProjection,
            saved_tokens,
            None,
        ));
    }

    pub fn record_event(&mut self, event: TokenSavingEvent) {
        let saved_tokens = event.saved_tokens();
        if !event.reversible || saved_tokens == 0 {
            return;
        }
        self.last_saved_tokens = saved_tokens;
        self.total_saved_tokens = self.total_saved_tokens.saturating_add(saved_tokens);
        if let Some(category) = self
            .categories
            .iter_mut()
            .find(|usage| usage.kind == event.kind)
        {
            category.last_saved_tokens = saved_tokens;
            category.total_saved_tokens = category.total_saved_tokens.saturating_add(saved_tokens);
            category.occurrences = category.occurrences.saturating_add(1);
        } else {
            self.categories.push(TokenSavingCategoryUsage {
                kind: event.kind,
                total_saved_tokens: saved_tokens,
                last_saved_tokens: saved_tokens,
                occurrences: 1,
            });
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum TokenSavingKind {
    OutputRepetition,
    OutputDelta,
    ArtifactProjection,
    UnchangedResource,
    SearchDelta,
    ToolSchemaElision,
    WorkingStateProjection,
    ToolOutputProjection,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct TokenSavingEvent {
    pub kind: TokenSavingKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub original_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub sent_tokens: i64,
    #[serde(default)]
    pub reversible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reference: Option<String>,
}

impl TokenSavingEvent {
    pub fn new(
        kind: TokenSavingKind,
        original_tokens: i64,
        sent_tokens: i64,
        reversible: bool,
        reference: Option<String>,
    ) -> Self {
        Self {
            kind,
            original_tokens: original_tokens.max(0),
            sent_tokens: sent_tokens.max(0),
            reversible,
            reference,
        }
    }

    pub fn reversible(kind: TokenSavingKind, saved_tokens: i64, reference: Option<String>) -> Self {
        Self::new(kind, saved_tokens, 0, true, reference)
    }

    pub fn saved_tokens(&self) -> i64 {
        self.original_tokens.saturating_sub(self.sent_tokens).max(0)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct TokenSavingCategoryUsage {
    pub kind: TokenSavingKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub total_saved_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub last_saved_tokens: i64,
    #[serde(default)]
    #[ts(type = "number")]
    pub occurrences: i64,
}

// Includes prompts, tools and space to call compact.
const BASELINE_TOKENS: i64 = 12000;

impl TokenUsage {
    pub fn is_zero(&self) -> bool {
        self.total_tokens == 0
    }

    pub fn cached_input(&self) -> i64 {
        self.cached_input_tokens.max(0)
    }

    pub fn cache_reported_input(&self) -> i64 {
        let input_tokens = self.input_tokens.max(0);
        let reported_input_tokens = self.cache_reported_input_tokens.max(0);
        if input_tokens == 0 {
            reported_input_tokens
        } else if reported_input_tokens == 0 {
            0
        } else {
            reported_input_tokens.min(input_tokens)
        }
    }

    pub fn non_cached_input(&self) -> i64 {
        (self.input_tokens - self.cached_input()).max(0)
    }

    pub fn cache_hit_percent(&self) -> Option<i64> {
        let cache_reported_input = self.cache_reported_input();
        if cache_reported_input == 0 {
            return None;
        }

        let cached_input = self.cached_input().min(cache_reported_input);
        Some(
            ((cached_input as f64 / cache_reported_input as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as i64,
        )
    }

    /// Primary count for display as a single absolute value: non-cached input + output.
    pub fn blended_total(&self) -> i64 {
        (self.non_cached_input() + self.output_tokens.max(0)).max(0)
    }

    pub fn tokens_in_context_window(&self) -> i64 {
        let input_tokens = self.input_tokens.max(0);
        if input_tokens > 0 {
            input_tokens
        } else {
            self.total_tokens.max(0)
        }
    }

    /// Estimate the remaining user-controllable percentage of the model's context window.
    ///
    /// `context_window` is the total size of the model's context window.
    /// `BASELINE_TOKENS` should capture tokens that are always present in
    /// the context (e.g., system prompt and fixed tool instructions) so that
    /// the percentage reflects the portion the user can influence.
    ///
    /// This normalizes both the numerator and denominator by subtracting the
    /// baseline, so immediately after the first prompt the UI shows 100% left
    /// and trends toward 0% as the user fills the effective window.
    pub fn percent_of_context_window_remaining(&self, context_window: i64) -> i64 {
        if context_window <= BASELINE_TOKENS {
            return 0;
        }

        let effective_window = context_window - BASELINE_TOKENS;
        let used = (self.tokens_in_context_window() - BASELINE_TOKENS).max(0);
        let remaining = (effective_window - used).max(0);
        ((remaining as f64 / effective_window as f64) * 100.0)
            .clamp(0.0, 100.0)
            .round() as i64
    }

    /// In-place element-wise sum of token counts.
    pub fn add_assign(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.cache_reported_input_tokens += other.cache_reported_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FinalOutput {
    pub token_usage: TokenUsage,
}

impl From<TokenUsage> for FinalOutput {
    fn from(token_usage: TokenUsage) -> Self {
        Self { token_usage }
    }
}

impl fmt::Display for FinalOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token_usage = &self.token_usage;

        write!(
            f,
            "Token usage: total={} input={}{} output={}{}",
            format_with_separators(token_usage.blended_total()),
            format_with_separators(token_usage.non_cached_input()),
            if token_usage.cached_input() > 0 {
                format!(
                    " (+ {} cached)",
                    format_with_separators(token_usage.cached_input())
                )
            } else {
                String::new()
            },
            format_with_separators(token_usage.output_tokens),
            if token_usage.reasoning_output_tokens > 0 {
                format!(
                    " (reasoning {})",
                    format_with_separators(token_usage.reasoning_output_tokens)
                )
            } else {
                String::new()
            }
        )
    }
}
