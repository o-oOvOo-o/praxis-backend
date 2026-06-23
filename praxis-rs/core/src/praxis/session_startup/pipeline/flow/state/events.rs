use praxis_protocol::protocol::Event;

pub(in crate::praxis::session_startup::pipeline::flow) struct SessionStartupEvents {
    pub(in crate::praxis::session_startup::pipeline::flow) post_session_configured_events:
        Vec<Event>,
}
