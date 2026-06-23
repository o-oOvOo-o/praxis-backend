use std::sync::Arc;

use super::super::super::super::Session;
use super::super::super::services_bootstrap;
use super::SessionAssemblyInput;
use super::handle;

pub(in crate::praxis::session_startup::pipeline) async fn build(
    input: SessionAssemblyInput<'_>,
) -> anyhow::Result<Arc<Session>> {
    let assembly = input.into_assembly_parts();
    let services = services_bootstrap::build(assembly.services_input).await?;
    handle::build_and_bind(
        assembly
            .handle_seed
            .into_handle_input(assembly.state, services),
    )
    .await
}
