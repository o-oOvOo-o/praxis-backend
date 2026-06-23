use std::sync::Arc;

use super::super::Session;
use super::input::SessionStartupInput;

mod flow;
mod post_assembly;
mod session_assembly;
mod session_configured_emit;
mod session_runtime_prepare;
mod startup_artifacts;

pub(super) async fn run(input: SessionStartupInput) -> anyhow::Result<Arc<Session>> {
    flow::SessionStartupFlow::from(input).run().await
}
