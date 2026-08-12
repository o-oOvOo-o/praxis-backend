use praxis_app_gateway_protocol::PluginCommandExecuteParams;

#[test]
fn serializes_current_thread_context_for_plugin_processes() {
    let cwd = std::env::current_dir().expect("current directory must be available");
    let rollout_path = cwd.join("rollout.jsonl");
    let params: PluginCommandExecuteParams = serde_json::from_value(serde_json::json!({
        "pluginId": "praxis-thread-share",
        "commandName": "share",
        "args": [],
        "threadId": "thread-123",
        "rolloutPath": rollout_path,
        "cwd": cwd,
    }))
    .expect("deserialize plugin command params");

    let value = serde_json::to_value(params).expect("serialize plugin command params");

    assert_eq!(value["threadId"], "thread-123");
    assert!(value["rolloutPath"].as_str().is_some());
    assert!(value["cwd"].as_str().is_some());
}
