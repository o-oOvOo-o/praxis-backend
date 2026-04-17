use std::sync::Arc;

use codex_protocol::protocol::TokenUsage;
use tracing::warn;

use crate::codex::Session;

#[derive(Clone, Copy)]
struct ModelPricing {
    slug: &'static str,
    input_usd_per_million_micros: i64,
    cached_input_usd_per_million_micros: Option<i64>,
    output_usd_per_million_micros: i64,
}

// Aligned to OpenAI public pricing pages on 2026-04-03.
const MODEL_PRICING: &[ModelPricing] = &[
    ModelPricing {
        slug: "gpt-5.4-pro",
        input_usd_per_million_micros: 20_000_000,
        cached_input_usd_per_million_micros: None,
        output_usd_per_million_micros: 160_000_000,
    },
    ModelPricing {
        slug: "gpt-5.4",
        input_usd_per_million_micros: 2_000_000,
        cached_input_usd_per_million_micros: Some(500_000),
        output_usd_per_million_micros: 8_000_000,
    },
    ModelPricing {
        slug: "gpt-5.2-pro",
        input_usd_per_million_micros: 15_000_000,
        cached_input_usd_per_million_micros: None,
        output_usd_per_million_micros: 120_000_000,
    },
    ModelPricing {
        slug: "gpt-5.2-codex",
        input_usd_per_million_micros: 1_500_000,
        cached_input_usd_per_million_micros: Some(375_000),
        output_usd_per_million_micros: 6_000_000,
    },
    ModelPricing {
        slug: "gpt-5.2-chat-latest",
        input_usd_per_million_micros: 1_250_000,
        cached_input_usd_per_million_micros: Some(125_000),
        output_usd_per_million_micros: 10_000_000,
    },
    ModelPricing {
        slug: "gpt-5.2",
        input_usd_per_million_micros: 1_250_000,
        cached_input_usd_per_million_micros: Some(125_000),
        output_usd_per_million_micros: 10_000_000,
    },
    ModelPricing {
        slug: "gpt-5.1-codex-max",
        input_usd_per_million_micros: 2_000_000,
        cached_input_usd_per_million_micros: Some(500_000),
        output_usd_per_million_micros: 8_000_000,
    },
    ModelPricing {
        slug: "gpt-5.1-codex-mini",
        input_usd_per_million_micros: 400_000,
        cached_input_usd_per_million_micros: Some(100_000),
        output_usd_per_million_micros: 1_600_000,
    },
    ModelPricing {
        slug: "gpt-5.1-codex",
        input_usd_per_million_micros: 1_500_000,
        cached_input_usd_per_million_micros: Some(375_000),
        output_usd_per_million_micros: 6_000_000,
    },
    ModelPricing {
        slug: "gpt-5.1-chat-latest",
        input_usd_per_million_micros: 1_250_000,
        cached_input_usd_per_million_micros: Some(125_000),
        output_usd_per_million_micros: 10_000_000,
    },
    ModelPricing {
        slug: "gpt-5.1",
        input_usd_per_million_micros: 1_250_000,
        cached_input_usd_per_million_micros: Some(125_000),
        output_usd_per_million_micros: 10_000_000,
    },
    ModelPricing {
        slug: "gpt-5-pro",
        input_usd_per_million_micros: 15_000_000,
        cached_input_usd_per_million_micros: None,
        output_usd_per_million_micros: 120_000_000,
    },
    ModelPricing {
        slug: "gpt-5-codex",
        input_usd_per_million_micros: 1_500_000,
        cached_input_usd_per_million_micros: Some(375_000),
        output_usd_per_million_micros: 6_000_000,
    },
    ModelPricing {
        slug: "gpt-5-chat-latest",
        input_usd_per_million_micros: 1_250_000,
        cached_input_usd_per_million_micros: Some(125_000),
        output_usd_per_million_micros: 10_000_000,
    },
    ModelPricing {
        slug: "gpt-5-mini",
        input_usd_per_million_micros: 250_000,
        cached_input_usd_per_million_micros: Some(25_000),
        output_usd_per_million_micros: 2_000_000,
    },
    ModelPricing {
        slug: "gpt-5-nano",
        input_usd_per_million_micros: 50_000,
        cached_input_usd_per_million_micros: Some(5_000),
        output_usd_per_million_micros: 400_000,
    },
    ModelPricing {
        slug: "gpt-5",
        input_usd_per_million_micros: 1_250_000,
        cached_input_usd_per_million_micros: Some(125_000),
        output_usd_per_million_micros: 10_000_000,
    },
];

pub(crate) async fn persist_turn_cost_estimate(
    sess: &Arc<Session>,
    model_slug: &str,
    turn_token_usage: Option<&TokenUsage>,
) {
    let Some(turn_token_usage) = turn_token_usage else {
        return;
    };
    let Some(state_db) = sess.services.state_db.as_deref() else {
        return;
    };
    let Ok(Some(mut metadata)) = state_db.get_thread(sess.conversation_id).await else {
        return;
    };

    let last_cost_micros = estimate_turn_cost_micros(model_slug, turn_token_usage);
    if let Some(last_cost_micros) = last_cost_micros {
        metadata.last_cost_micros = Some(last_cost_micros);
        metadata.total_cost_micros = Some(
            metadata
                .total_cost_micros
                .unwrap_or(0)
                .saturating_add(last_cost_micros),
        );
    } else {
        metadata.last_cost_micros = None;
    }

    if let Err(err) = state_db.upsert_thread(&metadata).await {
        warn!(
            "failed to persist cost estimate for thread {}: {err:#}",
            sess.conversation_id
        );
    }
}

pub(crate) fn estimate_turn_cost_micros(model_slug: &str, usage: &TokenUsage) -> Option<i64> {
    let pricing = pricing_for_model(model_slug)?;
    let input_cost = component_cost_micros(
        usage.non_cached_input(),
        pricing.input_usd_per_million_micros,
    );
    let cached_cost = component_cost_micros(
        usage.cached_input(),
        pricing
            .cached_input_usd_per_million_micros
            .unwrap_or(pricing.input_usd_per_million_micros),
    );
    let output_cost = component_cost_micros(
        usage
            .output_tokens
            .saturating_add(usage.reasoning_output_tokens)
            .max(0),
        pricing.output_usd_per_million_micros,
    );

    Some(
        input_cost
            .saturating_add(cached_cost)
            .saturating_add(output_cost),
    )
}

fn pricing_for_model(model_slug: &str) -> Option<ModelPricing> {
    let normalized = normalize_model_slug(model_slug);
    MODEL_PRICING.iter().copied().find(|pricing| {
        normalized == pricing.slug || normalized.starts_with(&format!("{}-", pricing.slug))
    })
}

fn normalize_model_slug(model_slug: &str) -> String {
    model_slug
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or(model_slug)
        .to_ascii_lowercase()
}

fn component_cost_micros(tokens: i64, micros_per_million: i64) -> i64 {
    let tokens = tokens.max(0) as i128;
    let rate = micros_per_million.max(0) as i128;
    let micros = tokens.saturating_mul(rate) / 1_000_000_i128;
    micros.clamp(0, i64::MAX as i128) as i64
}
