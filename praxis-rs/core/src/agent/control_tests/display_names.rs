use super::*;

#[test]
fn thread_spawn_agent_display_name_combines_chinese_name_and_short_title() {
    let identity = build_agent_display_identity("墨子".to_string(), Some("负责GUI"));
    assert_eq!(identity.display_name, "墨子-负责GUI");
    assert_eq!(identity.title.as_deref(), Some("负责GUI"));

    let identity = build_agent_display_identity("墨子".to_string(), Some("墨子-负责GUI"));
    assert_eq!(identity.display_name, "墨子-负责GUI");
    assert_eq!(identity.title.as_deref(), Some("负责GUI"));

    let identity = build_agent_display_identity("庄子".to_string(), Some("墨子-负责GUI"));
    assert_eq!(identity.display_name, "庄子-负责GUI");
    assert_eq!(identity.title.as_deref(), Some("负责GUI"));
}

#[test]
fn thread_spawn_agent_display_name_drops_redundant_name_only_title() {
    let identity = build_agent_display_identity("荀子".to_string(), Some("荀子"));
    assert_eq!(identity.display_name, "荀子");
    assert_eq!(identity.title, None);

    let identity = build_agent_display_identity("荀子".to_string(), Some("荀子-荀子"));
    assert_eq!(identity.display_name, "荀子");
    assert_eq!(identity.title, None);

    let identity = build_agent_display_identity("Atlas".to_string(), Some("atlas"));
    assert_eq!(identity.display_name, "Atlas");
    assert_eq!(identity.title, None);
}

#[test]
fn listed_agent_next_action_uses_recommended_thread_target() {
    let target = "019e72db-3096-7c42-b00d-61f63e0ac96c";

    let action = listed_agent_next_action(target, &AgentStatus::Running);
    assert!(action.contains(target));
    assert!(action.contains("wait_agent"));

    let action = listed_agent_next_action(target, &AgentStatus::Completed(Some("done".into())));
    assert!(action.contains(target));
    assert!(action.contains("assign_task"));
    assert!(action.contains("close_agent"));
}
