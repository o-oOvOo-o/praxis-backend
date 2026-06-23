use std::sync::Arc;

use praxis_exec_server::Environment;
use praxis_exec_server::EnvironmentManager;

pub(super) async fn current(
    environment_manager: &Arc<EnvironmentManager>,
) -> anyhow::Result<Arc<Environment>> {
    Ok(environment_manager.current().await?)
}
