use super::*;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

#[test]
fn list_directory_tool_matches_expected_spec() {
    assert_eq!(
        create_list_directory_tool(),
        ToolSpec::Function(ResponsesApiTool {
            name: LIST_DIRECTORY_TOOL_NAME.to_string(),
            description: "Fast read-only directory navigation for local workspaces. Use this to inspect folder structure; use rg for filename/content search and shell commands only for diagnostics or execution.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([
                    (
                        "depth".to_string(),
                        JsonSchema::Number {
                            description: Some(
                                "The maximum directory depth to traverse. Must be 1 or greater."
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "max_entries".to_string(),
                        JsonSchema::Number {
                            description: Some(
                                "Hard cap on scanned entries before pagination. Use a narrower path or smaller depth before raising this."
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "respect_ignore".to_string(),
                        JsonSchema::Boolean {
                            description: Some(
                                "Whether to honor repository ignore rules such as .gitignore and .ignore. Defaults to true."
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "include_hidden".to_string(),
                        JsonSchema::Boolean {
                            description: Some(
                                "Whether to include hidden files and directories. Defaults to false."
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "kind".to_string(),
                        JsonSchema::String {
                            description: Some(
                                "Entry kind filter: all, directories, or files. Defaults to all."
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "path".to_string(),
                        JsonSchema::String {
                            description: Some(
                                "Workspace-relative or absolute directory path to list. Prefer a relative path inside the current project."
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "limit".to_string(),
                        JsonSchema::Number {
                            description: Some(
                                "The maximum number of entries to return.".to_string(),
                            ),
                        },
                    ),
                    (
                        "offset".to_string(),
                        JsonSchema::Number {
                            description: Some(
                                "The entry number to start listing from. Must be 1 or greater."
                                    .to_string(),
                            ),
                        },
                    ),
                ]),
                required: Some(vec!["path".to_string()]),
                additional_properties: Some(false.into()),
            },
            output_schema: None,
        })
    );
}

#[test]
fn test_sync_tool_matches_expected_spec() {
    assert_eq!(
        create_test_sync_tool(),
        ToolSpec::Function(ResponsesApiTool {
            name: "test_sync_tool".to_string(),
            description: "Internal synchronization helper used by Praxis integration tests."
                .to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([
                    (
                        "barrier".to_string(),
                        JsonSchema::Object {
                            properties: BTreeMap::from([
                                (
                                    "id".to_string(),
                                    JsonSchema::String {
                                        description: Some(
                                            "Identifier shared by concurrent calls that should rendezvous"
                                                .to_string(),
                                        ),
                                    },
                                ),
                                (
                                    "participants".to_string(),
                                    JsonSchema::Number {
                                        description: Some(
                                            "Number of tool calls that must arrive before the barrier opens"
                                                .to_string(),
                                        ),
                                    },
                                ),
                                (
                                    "timeout_ms".to_string(),
                                    JsonSchema::Number {
                                        description: Some(
                                            "Maximum time in milliseconds to wait at the barrier"
                                                .to_string(),
                                        ),
                                    },
                                ),
                            ]),
                            required: Some(vec![
                                "id".to_string(),
                                "participants".to_string(),
                            ]),
                            additional_properties: Some(false.into()),
                        },
                    ),
                    (
                        "sleep_after_ms".to_string(),
                        JsonSchema::Number {
                            description: Some(
                                "Optional delay in milliseconds after completing the barrier"
                                    .to_string(),
                            ),
                        },
                    ),
                    (
                        "sleep_before_ms".to_string(),
                        JsonSchema::Number {
                            description: Some(
                                "Optional delay in milliseconds before any other action"
                                    .to_string(),
                            ),
                        },
                    ),
                ]),
                required: None,
                additional_properties: Some(false.into()),
            },
            output_schema: None,
        })
    );
}
