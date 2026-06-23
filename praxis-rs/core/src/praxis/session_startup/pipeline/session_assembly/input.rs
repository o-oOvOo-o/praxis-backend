mod handle;
mod managers;
mod runtime;

pub(in crate::praxis::session_startup::pipeline) use handle::SessionAssemblyHandle;
pub(in crate::praxis::session_startup::pipeline) use managers::SessionAssemblyManagers;
pub(in crate::praxis::session_startup::pipeline) use runtime::SessionAssemblyRuntime;

pub(in crate::praxis::session_startup::pipeline) struct SessionAssemblyInput<'a> {
    pub(in crate::praxis::session_startup::pipeline) handle: SessionAssemblyHandle<'a>,
    pub(in crate::praxis::session_startup::pipeline) managers: SessionAssemblyManagers<'a>,
    pub(in crate::praxis::session_startup::pipeline) runtime: SessionAssemblyRuntime,
}
