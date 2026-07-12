use super::*;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use pretty_assertions::assert_eq;
use serde_json::json;

fn model_preset(id: &str, show_in_picker: bool) -> ModelPreset {
    ModelPreset {
        id: id.to_string(),
        model: format!("{id}-model"),
        display_name: format!("{id} display"),
        description: format!("{id} description"),
        default_reasoning_effort: ReasoningEffort::Medium,
        supported_reasoning_efforts: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::Medium,
            description: "Balanced".to_string(),
        }],
        supports_personality: false,
        is_default: false,
        upgrade: None,
        show_in_picker,
        availability_nux: None,
        supported_in_api: true,
        input_modalities: Vec::new(),
    }
}

fn string_property_description(properties: &BTreeMap<String, JsonSchema>, name: &str) -> String {
    match properties.get(name) {
        Some(JsonSchema::String {
            description: Some(description),
        }) => description.clone(),
        other => panic!("expected string property `{name}` with description, got {other:?}"),
    }
}

#[test]
fn spawn_agent_tool_requires_task_name_and_lists_visible_models() {
    let tool = create_spawn_agent_tool(SpawnAgentToolOptions {
        available_models: &[
            model_preset("visible", /*show_in_picker*/ true),
            model_preset("hidden", /*show_in_picker*/ false),
        ],
        agent_type_description: "role help".to_string(),
        multi_agent_mode: &praxis_protocol::config_types::MultiAgentMode::ExplicitRequestOnly,
    });

    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        output_schema,
        ..
    }) = tool
    else {
        panic!("spawn_agent should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("spawn_agent should use object params");
    };
    assert!(description.contains("visible display (`visible-model`)"));
    assert!(!description.contains("hidden display (`hidden-model`)"));
    assert!(description.contains("Cross-provider coding workers"));
    assert!(description.contains("`model_provider` to `openai`"));
    assert!(description.contains("`model` to `gpt-5.5`"));
    assert!(description.contains("`reasoning_effort` to `xhigh`"));
    assert!(properties.contains_key("task_name"));
    assert!(properties.contains_key("title"));
    assert!(properties.contains_key("message"));
    assert!(properties.contains_key("fork_turns"));
    assert!(properties.contains_key("model_provider"));
    assert!(!properties.contains_key("items"));
    assert!(!properties.contains_key("fork_context"));
    assert_eq!(
        properties.get("agent_type"),
        Some(&JsonSchema::String {
            description: Some("role help".to_string()),
        })
    );
    assert_eq!(
        required,
        Some(vec!["task_name".to_string(), "message".to_string()])
    );
    assert_eq!(
        output_schema.expect("spawn_agent output schema")["required"],
        json!([
            "agent_id",
            "task_name",
            "agent_base_name",
            "agent_title",
            "agent_display_name",
            "recommended_target",
            "next_action"
        ])
    );
}

#[test]
fn spawn_agent_tool_uses_proactive_policy_for_ultra_mode() {
    let tool = create_spawn_agent_tool(SpawnAgentToolOptions {
        available_models: &[],
        agent_type_description: String::new(),
        multi_agent_mode: &praxis_protocol::config_types::MultiAgentMode::Proactive,
    });
    let ToolSpec::Function(ResponsesApiTool { description, .. }) = tool else {
        panic!("spawn_agent should be a function tool");
    };
    assert!(description.contains("Proactive multi-agent delegation is active"));
    assert!(description.contains("no explicit user request is required"));
    assert!(!description.contains("Only use `spawn_agent` if and only if"));
}

#[test]
fn send_message_tool_requires_message_and_uses_submission_output() {
    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        output_schema,
        ..
    }) = create_send_message_tool()
    else {
        panic!("send_message should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("send_message should use object params");
    };
    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("message"));
    let target_description = string_property_description(&properties, "target");
    assert!(target_description.contains("Prefer `recommended_target`"));
    assert!(target_description.contains("Do not use `agent_name`"));
    assert!(!properties.contains_key("interrupt"));
    assert!(!properties.contains_key("items"));
    assert!(description.contains("without triggering a new turn"));
    assert!(description.contains("use assign_task"));
    assert_eq!(
        required,
        Some(vec!["target".to_string(), "message".to_string()])
    );
    assert_eq!(
        output_schema.expect("send_message output schema")["required"],
        json!([
            "submission_id",
            "runtime_command_id",
            "target",
            "target_thread_id",
            "target_agent_base_name",
            "target_agent_title",
            "target_agent_display_name",
            "delivery_mode",
            "next_action"
        ])
    );
}

#[test]
fn assign_task_tool_requires_structured_task_and_uses_submission_output() {
    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        output_schema,
        ..
    }) = create_assign_task_tool()
    else {
        panic!("assign_task should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("assign_task should use object params");
    };
    assert!(properties.contains_key("target"));
    let target_description = string_property_description(&properties, "target");
    assert!(target_description.contains("Prefer `recommended_target`"));
    assert!(target_description.contains("canonical task names"));
    assert!(properties.contains_key("objective"));
    assert!(properties.contains_key("message"));
    assert!(properties.contains_key("scope"));
    assert!(properties.contains_key("constraints"));
    assert!(properties.contains_key("acceptance_criteria"));
    assert!(properties.contains_key("artifact_refs"));
    assert!(properties.contains_key("required_capabilities"));
    assert!(properties.contains_key("required_resources"));
    assert!(properties.contains_key("token_budget"));
    assert!(properties.contains_key("priority"));
    assert!(properties.contains_key("exploratory"));
    assert!(properties.contains_key("interrupt"));
    assert!(!properties.contains_key("items"));
    assert!(description.contains("trigger a new turn"));
    assert!(description.contains("not send_message"));
    assert_eq!(
        required,
        Some(vec![
            "target".to_string(),
            "objective".to_string(),
            "scope".to_string()
        ])
    );
    assert_eq!(
        output_schema.expect("assign_task output schema")["required"],
        json!([
            "submission_id",
            "runtime_command_id",
            "target",
            "target_thread_id",
            "target_agent_base_name",
            "target_agent_title",
            "target_agent_display_name",
            "delivery_mode",
            "next_action"
        ])
    );
}

#[test]
fn wait_agent_tool_accepts_optional_target_and_returns_target_status() {
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = create_wait_agent_tool(WaitAgentTimeoutOptions {
        default_timeout_ms: 30_000,
        min_timeout_ms: 10_000,
        max_timeout_ms: 3_600_000,
    })
    else {
        panic!("wait_agent should be a function tool");
    };
    let JsonSchema::Object {
        properties,
        required,
        ..
    } = parameters
    else {
        panic!("wait_agent should use object params");
    };
    assert!(!properties.contains_key("targets"));
    assert!(properties.contains_key("target"));
    let target_description = string_property_description(&properties, "target");
    assert!(target_description.contains("Prefer `recommended_target`"));
    assert!(target_description.contains("thread id"));
    assert!(properties.contains_key("timeout_ms"));
    assert_eq!(required, None);
    let output_schema = output_schema.expect("wait output schema");
    assert_eq!(
        output_schema["properties"]["message"]["description"],
        json!("Brief wait summary.")
    );
    assert_eq!(
        output_schema["properties"]["source"]["enum"],
        json!(["mailbox", "agent_os", "target_status", "timeout"])
    );
    assert_eq!(
        output_schema["properties"]["target_status"]["allOf"][0]["oneOf"][0]["enum"],
        json!([
            "pending_init",
            "running",
            "interrupted",
            "shutdown",
            "not_found"
        ])
    );
    assert_eq!(
        output_schema["properties"]["next_action"]["description"],
        json!("Plain-language next step for worker coordination.")
    );
}

#[test]
fn list_agents_tool_includes_path_prefix_and_agent_fields() {
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = create_list_agents_tool()
    else {
        panic!("list_agents should be a function tool");
    };
    let JsonSchema::Object { properties, .. } = parameters else {
        panic!("list_agents should use object params");
    };
    assert!(properties.contains_key("path_prefix"));
    assert_eq!(
        output_schema.expect("list_agents output schema")["properties"]["agents"]["items"]["required"],
        json!([
            "thread_id",
            "recommended_target",
            "next_action",
            "agent_name",
            "agent_base_name",
            "agent_title",
            "agent_display_name",
            "agent_role",
            "agent_status",
            "last_task_message"
        ])
    );
}

#[test]
fn list_agents_tool_schema_includes_terminal_state_hint() {
    let ToolSpec::Function(ResponsesApiTool { output_schema, .. }) = create_list_agents_tool()
    else {
        panic!("list_agents should be a function tool");
    };

    let output_schema = output_schema.expect("list_agents output schema");
    assert_eq!(
        output_schema["required"],
        json!(["agents", "agent_os", "terminal_state"])
    );
    assert_eq!(
        output_schema["properties"]["terminal_state"]["properties"]["should_stop_listing"]["description"],
        json!("True when repeated list_agents calls are useless; summarize instead.")
    );
}

#[test]
fn list_agents_tool_status_schema_includes_interrupted() {
    let ToolSpec::Function(ResponsesApiTool { output_schema, .. }) = create_list_agents_tool()
    else {
        panic!("list_agents should be a function tool");
    };

    assert_eq!(
        output_schema.expect("list_agents output schema")["properties"]["agents"]["items"]["properties"]
            ["agent_status"]["allOf"][0]["oneOf"][0]["enum"],
        json!([
            "pending_init",
            "running",
            "interrupted",
            "shutdown",
            "not_found"
        ])
    );
}
