use crate::state::SessionState;

use super::super::super::services_bootstrap;
use super::handle_seed::SessionHandleSeed;

pub(super) struct SessionAssemblyParts<'a> {
    pub(super) state: SessionState,
    pub(super) services_input: services_bootstrap::ServicesBootstrapInput,
    pub(super) handle_seed: SessionHandleSeed<'a>,
}
