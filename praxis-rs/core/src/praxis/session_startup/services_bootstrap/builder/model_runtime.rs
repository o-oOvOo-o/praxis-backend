use std::sync::Arc;

use praxis_features::Feature;
use praxis_login::AuthManager;
use praxis_protocol::ThreadId;

use crate::client::ModelRuntimeRegistry;
use crate::config::Config;
use crate::llm::local_models::NativeLocalModelConfig;
use crate::praxis::SessionConfiguration;

use super::super::super::beta_features;

pub(super) fn build(
    config: &Config,
    auth_manager: &Arc<AuthManager>,
    conversation_id: ThreadId,
    session_configuration: &SessionConfiguration,
) -> ModelRuntimeRegistry {
    ModelRuntimeRegistry::new(
        Some(Arc::clone(auth_manager)),
        conversation_id,
        session_configuration.session_source.clone(),
        config.model_verbosity,
        config.features.enabled(Feature::EnableRequestCompression),
        config.features.enabled(Feature::RuntimeMetrics),
        beta_features::model_client_beta_features_header(config),
        NativeLocalModelConfig::from_config(config),
    )
}
