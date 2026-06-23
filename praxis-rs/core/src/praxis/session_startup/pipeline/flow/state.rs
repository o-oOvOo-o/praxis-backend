mod channels;
mod control;
mod events;
mod services;
mod spec;

use super::super::super::input::SessionStartupInput;
pub(super) use channels::SessionStartupChannels;
pub(super) use control::SessionStartupControl;
pub(super) use events::SessionStartupEvents;
pub(super) use services::SessionStartupServices;
pub(super) use spec::SessionStartupSpec;

pub(in crate::praxis::session_startup::pipeline) struct SessionStartupFlow {
    pub(super) spec: SessionStartupSpec,
    pub(super) services: SessionStartupServices,
    pub(super) control: SessionStartupControl,
    pub(super) channels: SessionStartupChannels,
    pub(super) events: SessionStartupEvents,
}

impl From<SessionStartupInput> for SessionStartupFlow {
    fn from(input: SessionStartupInput) -> Self {
        let SessionStartupInput {
            session_configuration,
            llm_runtime_catalog,
            config,
            auth_manager,
            models_manager,
            exec_policy,
            tx_event,
            agent_status,
            initial_history,
            session_source,
            environment_manager,
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
            agent_control,
            agent_os,
        } = input;

        Self {
            spec: SessionStartupSpec {
                session_configuration,
                llm_runtime_catalog,
                initial_history,
                session_source,
            },
            services: SessionStartupServices {
                config,
                auth_manager,
                models_manager,
                exec_policy,
                environment_manager,
                skills_manager,
                plugins_manager,
                mcp_manager,
                skills_watcher,
            },
            control: SessionStartupControl {
                agent_control,
                agent_os,
            },
            channels: SessionStartupChannels {
                tx_event,
                agent_status,
            },
            events: SessionStartupEvents {
                post_session_configured_events: Vec::new(),
            },
        }
    }
}
