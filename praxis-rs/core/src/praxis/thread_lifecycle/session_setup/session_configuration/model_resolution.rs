use std::sync::Arc;

use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::protocol::SessionSource;

use crate::config::Config;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::manager::RefreshStrategy;

pub(super) struct ResolvedModelSelection {
    pub(super) model: String,
    pub(super) model_info: ModelInfo,
}

pub(super) async fn resolve(
    models_manager: &ModelsManager,
    config: &Arc<Config>,
    session_source: &SessionSource,
) -> ResolvedModelSelection {
    let refresh_strategy = refresh_strategy_for(session_source);
    if config.model.is_none() || !matches!(refresh_strategy, RefreshStrategy::Offline) {
        let _ = models_manager
            .list_models_for_config(config, refresh_strategy)
            .await;
    }
    let model = models_manager
        .get_default_model_for_config(&config.model, refresh_strategy, config)
        .await;
    let model_info = models_manager.get_model_info(model.as_str(), config).await;
    ResolvedModelSelection { model, model_info }
}

fn refresh_strategy_for(session_source: &SessionSource) -> RefreshStrategy {
    if matches!(session_source, SessionSource::SubAgent(_)) {
        RefreshStrategy::Offline
    } else {
        RefreshStrategy::OnlineIfUncached
    }
}
