use std::sync::Arc;

use praxis_config::CONFIG_TOML_FILE;
use praxis_utils_absolute_path::AbsolutePathBuf;
use tracing::warn;

use crate::ModelProviderInfo;
use crate::config::Config;
use crate::session_startup_prewarm::SessionStartupPrewarmHandle;

use super::super::Session;

impl Session {
    pub(crate) async fn set_session_startup_prewarm(
        &self,
        startup_prewarm: SessionStartupPrewarmHandle,
    ) {
        let mut state = self.state.lock().await;
        state.set_session_startup_prewarm(startup_prewarm);
    }

    pub(crate) async fn take_session_startup_prewarm(&self) -> Option<SessionStartupPrewarmHandle> {
        let mut state = self.state.lock().await;
        state.take_session_startup_prewarm()
    }

    pub(crate) async fn get_config(&self) -> Arc<Config> {
        let state = self.state.lock().await;
        state
            .session_configuration
            .original_config_do_not_use
            .clone()
    }

    pub(crate) async fn provider(&self) -> ModelProviderInfo {
        let state = self.state.lock().await;
        state.session_configuration.provider.clone()
    }

    pub(crate) async fn reload_user_config_layer(&self) {
        let config_toml_path = {
            let state = self.state.lock().await;
            state
                .session_configuration
                .praxis_home
                .join(CONFIG_TOML_FILE)
        };

        let user_config = match std::fs::read_to_string(&config_toml_path) {
            Ok(contents) => match toml::from_str::<toml::Value>(&contents) {
                Ok(config) => config,
                Err(err) => {
                    warn!("failed to parse user config while reloading layer: {err}");
                    return;
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                toml::Value::Table(Default::default())
            }
            Err(err) => {
                warn!("failed to read user config while reloading layer: {err}");
                return;
            }
        };

        let config_toml_path = match AbsolutePathBuf::try_from(config_toml_path) {
            Ok(path) => path,
            Err(err) => {
                warn!("failed to resolve user config path while reloading layer: {err}");
                return;
            }
        };

        let mut state = self.state.lock().await;
        let mut config = (*state.session_configuration.original_config_do_not_use).clone();
        config.config_layer_stack = config
            .config_layer_stack
            .with_user_config(&config_toml_path, user_config);
        state.session_configuration.original_config_do_not_use = Arc::new(config);
        self.services.skills_manager.clear_cache();
        self.services.plugins_manager.clear_cache();
    }
}
