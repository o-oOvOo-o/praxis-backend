use std::sync::Arc;

use praxis_protocol::protocol::SessionSource;

use crate::config::Config;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::exec_policy::ExecPolicyManager;

pub(super) async fn resolve(
    config: &Config,
    session_source: &SessionSource,
    inherited_exec_policy: Option<&Arc<ExecPolicyManager>>,
) -> PraxisResult<Arc<ExecPolicyManager>> {
    if crate::guardian::is_guardian_reviewer_source(session_source) {
        return Ok(Arc::new(ExecPolicyManager::default()));
    }

    if let Some(exec_policy) = inherited_exec_policy {
        return Ok(Arc::clone(exec_policy));
    }

    Ok(Arc::new(
        ExecPolicyManager::load(&config.config_layer_stack)
            .await
            .map_err(|err| PraxisErr::Fatal(format!("failed to load rules: {err}")))?,
    ))
}
