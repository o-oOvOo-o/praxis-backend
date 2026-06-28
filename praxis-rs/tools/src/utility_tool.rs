use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

pub const LIST_DIRECTORY_TOOL_NAME: &str = "list_directory";

pub fn create_list_directory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "path".to_string(),
            JsonSchema::String {
                description: Some(
                    "Workspace-relative or absolute directory path to list. Prefer a relative path inside the current project.".to_string(),
                ),
            },
        ),
        (
            "offset".to_string(),
            JsonSchema::Number {
                description: Some(
                    "The entry number to start listing from. Must be 1 or greater.".to_string(),
                ),
            },
        ),
        (
            "limit".to_string(),
            JsonSchema::Number {
                description: Some("The maximum number of entries to return.".to_string()),
            },
        ),
        (
            "depth".to_string(),
            JsonSchema::Number {
                description: Some(
                    "The maximum directory depth to traverse. Must be 1 or greater.".to_string(),
                ),
            },
        ),
        (
            "max_entries".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Hard cap on scanned entries before pagination. Use a narrower path or smaller depth before raising this.".to_string(),
                ),
            },
        ),
        (
            "respect_ignore".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "Whether to honor repository ignore rules such as .gitignore and .ignore. Defaults to true.".to_string(),
                ),
            },
        ),
        (
            "include_hidden".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "Whether to include hidden files and directories. Defaults to false.".to_string(),
                ),
            },
        ),
        (
            "kind".to_string(),
            JsonSchema::String {
                description: Some(
                    "Entry kind filter: all, directories, or files. Defaults to all.".to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: LIST_DIRECTORY_TOOL_NAME.to_string(),
        description: "Fast read-only directory navigation for local workspaces. Use this to inspect folder structure; use rg for filename/content search and shell commands only for diagnostics or execution.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["path".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

pub fn create_test_sync_tool() -> ToolSpec {
    let barrier_properties = BTreeMap::from([
        (
            "id".to_string(),
            JsonSchema::String {
                description: Some(
                    "Identifier shared by concurrent calls that should rendezvous".to_string(),
                ),
            },
        ),
        (
            "participants".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Number of tool calls that must arrive before the barrier opens".to_string(),
                ),
            },
        ),
        (
            "timeout_ms".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Maximum time in milliseconds to wait at the barrier".to_string(),
                ),
            },
        ),
    ]);

    let properties = BTreeMap::from([
        (
            "sleep_before_ms".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Optional delay in milliseconds before any other action".to_string(),
                ),
            },
        ),
        (
            "sleep_after_ms".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Optional delay in milliseconds after completing the barrier".to_string(),
                ),
            },
        ),
        (
            "barrier".to_string(),
            JsonSchema::Object {
                properties: barrier_properties,
                required: Some(vec!["id".to_string(), "participants".to_string()]),
                additional_properties: Some(false.into()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "test_sync_tool".to_string(),
        description: "Internal synchronization helper used by Praxis integration tests."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

#[cfg(test)]
#[path = "utility_tool_tests.rs"]
mod tests;
