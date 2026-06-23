use std::sync::Arc;

use super::super::super::parallel_startup;
use super::super::super::rollout_bootstrap;
use super::StartupArtifacts;
use super::StartupArtifactsInput;
use super::composition;

pub(in crate::praxis::session_startup::pipeline) async fn collect(
    input: StartupArtifactsInput<'_>,
) -> anyhow::Result<StartupArtifacts> {
    let rollout_bootstrap::RolloutBootstrap {
        conversation_id,
        forked_from_id,
        params: rollout_params,
        state_builder,
    } = rollout_bootstrap::build(
        input.initial_history,
        input.session_configuration,
        input.session_source,
    );

    let parallel = parallel_startup::run(
        Arc::clone(input.config),
        Arc::clone(input.auth_manager),
        Arc::clone(input.mcp_manager),
        input.session_configuration,
        rollout_params,
        state_builder,
    )
    .await?;

    Ok(composition::compose(
        composition::StartupArtifactCompositionInput {
            conversation_id,
            forked_from_id,
            parallel,
        },
    ))
}
