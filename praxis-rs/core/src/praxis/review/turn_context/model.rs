use std::sync::Arc;

use praxis_features::Feature;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::openai_models::ModelInfo;

use crate::config::Config;
use crate::config::ManagedFeatures;

use super::super::super::Session;
use super::super::super::TurnContext;

pub(super) fn select_review_model(
    config: &Arc<Config>,
    parent_turn_context: &Arc<TurnContext>,
) -> String {
    config
        .review_model
        .clone()
        .unwrap_or_else(|| parent_turn_context.model_info.slug.clone())
}

pub(super) async fn load_review_model_info(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    model: &str,
) -> ModelInfo {
    sess.services
        .models_manager
        .get_model_info(model, config)
        .await
}

pub(super) fn review_features(sess: &Session) -> ManagedFeatures {
    let mut review_features = sess.features.clone();
    let _ = review_features.disable(Feature::WebSearchRequest);
    let _ = review_features.disable(Feature::WebSearchCached);
    review_features
}

pub(super) fn review_web_search_mode() -> WebSearchMode {
    WebSearchMode::Disabled
}

pub(super) fn build_per_turn_config(
    config: &Arc<Config>,
    model: &str,
    review_features: ManagedFeatures,
    review_web_search_mode: WebSearchMode,
) -> Config {
    let mut per_turn_config = (**config).clone();
    per_turn_config.model = Some(model.to_string());
    per_turn_config.features = review_features;
    if let Err(err) = per_turn_config.web_search_mode.set(review_web_search_mode) {
        let fallback_value = per_turn_config.web_search_mode.value();
        tracing::warn!(
            error = %err,
            ?review_web_search_mode,
            ?fallback_value,
            "review web_search_mode is disallowed by requirements; keeping constrained value"
        );
    }
    per_turn_config
}
