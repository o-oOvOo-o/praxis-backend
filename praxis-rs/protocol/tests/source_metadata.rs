use praxis_protocol::ThreadId;
use praxis_protocol::protocol::{SessionSource, SubAgentSource};
#[test]
fn metadata_overlay_preserves_existing_values_and_parent() {
    let source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: ThreadId::new(),
        depth: 1,
        agent_path: None,
        agent_base_name: Some("worker".into()),
        agent_title: None,
        agent_display_name: None,
        agent_role: Some("reviewer".into()),
    });
    let mut expected = source.clone();
    if let SessionSource::SubAgent(SubAgentSource::ThreadSpawn { agent_title, .. }) = &mut expected
    {
        *agent_title = Some("Audit".into());
    }
    assert_eq!(
        source.with_thread_spawn_agent_metadata(None, Some("Audit".into()), None, None),
        expected
    );
    assert_eq!(
        SessionSource::Unknown.with_thread_spawn_agent_metadata(Some("x".into()), None, None, None),
        SessionSource::Unknown
    );
}
