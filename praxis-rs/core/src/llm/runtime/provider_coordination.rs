use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProviderRuntimeKey {
    provider_id: String,
    endpoint: Option<String>,
}

#[derive(Debug, Default)]
struct ProviderRuntimeState {
    cooldowns: HashMap<ProviderRuntimeKey, Instant>,
}

static COORDINATOR: LazyLock<Mutex<ProviderRuntimeState>> =
    LazyLock::new(|| Mutex::new(ProviderRuntimeState::default()));

pub(crate) async fn wait_until_ready(
    provider_id: &str,
    endpoint: Option<&str>,
    cancellation: &CancellationToken,
) -> bool {
    let key = ProviderRuntimeKey::new(provider_id, endpoint);
    loop {
        let deadline = {
            let mut state = COORDINATOR.lock().await;
            let now = Instant::now();
            state.cooldowns.retain(|_, deadline| *deadline > now);
            state.cooldowns.get(&key).copied()
        };
        let Some(deadline) = deadline else {
            return true;
        };
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {}
            _ = cancellation.cancelled() => return false,
        }
    }
}

pub(crate) async fn observe_rate_limit(provider_id: &str, endpoint: Option<&str>, delay: Duration) {
    let key = ProviderRuntimeKey::new(provider_id, endpoint);
    let deadline = Instant::now() + delay;
    let mut state = COORDINATOR.lock().await;
    state
        .cooldowns
        .entry(key)
        .and_modify(|current| *current = (*current).max(deadline))
        .or_insert(deadline);
}

impl ProviderRuntimeKey {
    fn new(provider_id: &str, endpoint: Option<&str>) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            endpoint: endpoint.map(str::to_string),
        }
    }
}
