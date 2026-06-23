use tracing::Instrument;

use crate::error::Result as PraxisResult;

use super::super::Praxis;
use super::PraxisSpawnArgs;
use super::PraxisSpawnOk;
use super::trace;

mod channels;
mod flow;
mod loop_spawn;
mod session_factory;

impl Praxis {
    /// Spawn a new [`Praxis`] and initialize the session.
    pub(crate) async fn spawn(mut args: PraxisSpawnArgs) -> PraxisResult<PraxisSpawnOk> {
        args.parent_trace = trace::valid_parent_trace(args.parent_trace);
        let thread_spawn_span = trace::thread_spawn_span(args.parent_trace.as_ref());
        Self::spawn_internal(args)
            .instrument(thread_spawn_span)
            .await
    }

    async fn spawn_internal(args: PraxisSpawnArgs) -> PraxisResult<PraxisSpawnOk> {
        flow::SpawnFlow::from(args).run().await
    }
}
