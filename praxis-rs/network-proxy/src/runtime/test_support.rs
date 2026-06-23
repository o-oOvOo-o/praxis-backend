use super::*;

#[cfg(test)]
pub(crate) fn network_proxy_state_for_policy(
    mut network: crate::config::NetworkProxySettings,
) -> NetworkProxyState {
    network.enabled = true;
    network.mode = NetworkMode::Full;
    let config = NetworkProxyConfig { network };
    let state = build_config_state(config, NetworkProxyConstraints::default()).unwrap();

    NetworkProxyState::with_reloader(state, Arc::new(NoopReloader))
}

#[cfg(test)]
pub(super) struct NoopReloader;

#[cfg(test)]
#[async_trait]
impl ConfigReloader for NoopReloader {
    fn source_label(&self) -> String {
        "test config state".to_string()
    }

    async fn maybe_reload(&self) -> Result<Option<ConfigState>> {
        Ok(None)
    }

    async fn reload_now(&self) -> Result<ConfigState> {
        Err(anyhow::anyhow!("force reload is not supported in tests"))
    }
}
