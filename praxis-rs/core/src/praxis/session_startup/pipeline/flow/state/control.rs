use std::sync::Arc;

use crate::agent::AgentControl;
use crate::agent_os::AgentOs;

pub(in crate::praxis::session_startup::pipeline::flow) struct SessionStartupControl {
    pub(in crate::praxis::session_startup::pipeline::flow) agent_control: AgentControl,
    pub(in crate::praxis::session_startup::pipeline::flow) agent_os: Arc<AgentOs>,
}
