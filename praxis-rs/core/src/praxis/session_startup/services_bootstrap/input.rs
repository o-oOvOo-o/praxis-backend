mod managers;
mod runtime;
mod session;

pub(in crate::praxis::session_startup) use managers::ServiceManagerSet;
pub(in crate::praxis::session_startup) use runtime::ServiceRuntimeArtifacts;
pub(in crate::praxis::session_startup) use session::ServiceSessionSpec;

pub(in crate::praxis::session_startup) struct ServicesBootstrapInput {
    pub(in crate::praxis::session_startup) session: ServiceSessionSpec,
    pub(in crate::praxis::session_startup) managers: ServiceManagerSet,
    pub(in crate::praxis::session_startup) runtime: ServiceRuntimeArtifacts,
}
