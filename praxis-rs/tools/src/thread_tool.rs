use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use praxis_protocol::protocol::AgentRank;
use serde_json::json;
use std::collections::BTreeMap;

pub const SPAWN_THREAD_TOOL_NAME: &str = "spawn_thread";

pub fn create_spawn_thread_tool(target_rank: AgentRank) -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "objective".to_string(),
            JsonSchema::String {
                description: Some(
                    "Durable objective for the new managed thread. Praxis persists it as the thread goal and starts goal execution automatically."
                        .to_string(),
                ),
            },
        ),
        (
            "task_name".to_string(),
            JsonSchema::String {
                description: Some(
                    "Stable lowercase task path segment used to identify this thread in the hierarchy."
                        .to_string(),
                ),
            },
        ),
        (
            "title".to_string(),
            JsonSchema::String {
                description: Some(
                    "Short human-facing responsibility label shown by Praxis clients."
                        .to_string(),
                ),
            },
        ),
        (
            "token_budget".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Optional positive token budget enforced by the persisted goal runtime."
                        .to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: SPAWN_THREAD_TOOL_NAME.to_string(),
        description: format!(
            "Create a managed {} thread, persist its goal, and start it independently. This is thread-level orchestration, not an ordinary spawn_agent subagent.",
            target_rank.label()
        ),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec![
                "objective".to_string(),
                "task_name".to_string(),
                "title".to_string(),
            ]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(json!({
            "type": "object",
            "properties": {
                "thread_id": { "type": "string" },
                "task_name": { "type": "string" },
                "title": { "type": "string" },
                "rank": { "type": "string", "enum": ["r1", "r2"] },
                "role": { "type": "string", "enum": ["supervisor", "worker"] },
                "goal": { "type": "object" },
                "model": { "type": "string" },
                "execution": { "type": "string", "enum": ["scheduled"] }
            },
            "required": ["thread_id", "task_name", "title", "rank", "role", "goal", "model", "execution"],
            "additionalProperties": false
        })),
    })
}
