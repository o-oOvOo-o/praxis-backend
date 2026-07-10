#[cfg(feature = "code_mode")]
use crate::FreeformTool;
#[cfg(feature = "code_mode")]
use crate::FreeformToolFormat;
#[cfg(feature = "code_mode")]
use crate::JsonSchema;
#[cfg(feature = "code_mode")]
use crate::ResponsesApiTool;
use crate::ToolSpec;
#[cfg(feature = "code_mode")]
use praxis_code_mode::CodeModeToolKind;
#[cfg(feature = "code_mode")]
use praxis_code_mode::ToolDefinition as CodeModeToolDefinition;
#[cfg(not(feature = "code_mode"))]
use serde_json::Value as JsonValue;
#[cfg(feature = "code_mode")]
use std::collections::BTreeMap;

pub const PUBLIC_TOOL_NAME: &str = "exec";
pub const WAIT_TOOL_NAME: &str = "wait";

#[cfg(not(feature = "code_mode"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeModeToolKind {
    Function,
    Freeform,
}

#[cfg(not(feature = "code_mode"))]
#[derive(Clone, Debug, PartialEq)]
pub struct CodeModeToolDefinition {
    pub name: String,
    pub description: String,
    pub kind: CodeModeToolKind,
    pub input_schema: Option<JsonValue>,
    pub output_schema: Option<JsonValue>,
}

/// Augment tool descriptions with code-mode-specific exec samples.
#[cfg(feature = "code_mode")]
pub fn augment_tool_spec_for_code_mode(spec: ToolSpec) -> ToolSpec {
    let Some(description) = code_mode_tool_definition_for_spec(&spec)
        .map(praxis_code_mode::augment_tool_definition)
        .map(|definition| definition.description)
    else {
        return spec;
    };

    match spec {
        ToolSpec::Function(mut tool) => {
            tool.description = description;
            ToolSpec::Function(tool)
        }
        ToolSpec::Freeform(mut tool) => {
            tool.description = description;
            ToolSpec::Freeform(tool)
        }
        other => other,
    }
}

#[cfg(not(feature = "code_mode"))]
pub fn augment_tool_spec_for_code_mode(spec: ToolSpec) -> ToolSpec {
    spec
}

/// Convert a supported nested tool spec into the code-mode runtime shape,
/// including the code-mode-specific description sample.
#[cfg(feature = "code_mode")]
pub fn tool_spec_to_code_mode_tool_definition(spec: &ToolSpec) -> Option<CodeModeToolDefinition> {
    let definition = code_mode_tool_definition_for_spec(spec)?;
    praxis_code_mode::is_code_mode_nested_tool(&definition.name)
        .then(|| praxis_code_mode::augment_tool_definition(definition))
}

#[cfg(not(feature = "code_mode"))]
pub fn tool_spec_to_code_mode_tool_definition(_spec: &ToolSpec) -> Option<CodeModeToolDefinition> {
    None
}

#[cfg(feature = "code_mode")]
pub fn collect_code_mode_tool_definitions<'a>(
    specs: impl IntoIterator<Item = &'a ToolSpec>,
) -> Vec<CodeModeToolDefinition> {
    let mut tool_definitions = specs
        .into_iter()
        .filter_map(tool_spec_to_code_mode_tool_definition)
        .collect::<Vec<_>>();
    tool_definitions.sort_by(|left, right| left.name.cmp(&right.name));
    tool_definitions.dedup_by(|left, right| left.name == right.name);
    tool_definitions
}

#[cfg(not(feature = "code_mode"))]
pub fn collect_code_mode_tool_definitions<'a>(
    _specs: impl IntoIterator<Item = &'a ToolSpec>,
) -> Vec<CodeModeToolDefinition> {
    Vec::new()
}

#[cfg(feature = "code_mode")]
pub fn create_wait_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "cell_id".to_string(),
            JsonSchema::String {
                description: Some("Identifier of the running exec cell.".to_string()),
            },
        ),
        (
            "yield_time_ms".to_string(),
            JsonSchema::Number {
                description: Some(
                    "How long to wait (in milliseconds) for more output before yielding again."
                        .to_string(),
                ),
            },
        ),
        (
            "max_tokens".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Maximum number of output tokens to return for this wait call.".to_string(),
                ),
            },
        ),
        (
            "terminate".to_string(),
            JsonSchema::Boolean {
                description: Some("Whether to terminate the running exec cell.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: praxis_code_mode::WAIT_TOOL_NAME.to_string(),
        description: format!(
            "Waits on a yielded `{}` cell and returns new output or completion.\n{}",
            praxis_code_mode::PUBLIC_TOOL_NAME,
            praxis_code_mode::build_wait_tool_description().trim()
        ),
        strict: false,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["cell_id".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
        defer_loading: None,
    })
}

#[cfg(not(feature = "code_mode"))]
pub fn create_wait_tool() -> ToolSpec {
    panic!("Code Mode wait tool is unavailable without the `code_mode` feature")
}

#[cfg(feature = "code_mode")]
pub fn create_code_mode_tool(
    enabled_tools: &[(String, String)],
    code_mode_only_enabled: bool,
) -> ToolSpec {
    const CODE_MODE_FREEFORM_GRAMMAR: &str = r#"
start: pragma_source | plain_source
pragma_source: PRAGMA_LINE NEWLINE SOURCE
plain_source: SOURCE

PRAGMA_LINE: /[ \t]*\/\/ @exec:[^\r\n]*/
NEWLINE: /\r?\n/
SOURCE: /[\s\S]+/
"#;

    ToolSpec::Freeform(FreeformTool {
        name: praxis_code_mode::PUBLIC_TOOL_NAME.to_string(),
        description: praxis_code_mode::build_exec_tool_description(
            enabled_tools,
            code_mode_only_enabled,
        ),
        format: FreeformToolFormat {
            r#type: "grammar".to_string(),
            syntax: "lark".to_string(),
            definition: CODE_MODE_FREEFORM_GRAMMAR.to_string(),
        },
    })
}

#[cfg(not(feature = "code_mode"))]
pub fn create_code_mode_tool(
    _enabled_tools: &[(String, String)],
    _code_mode_only_enabled: bool,
) -> ToolSpec {
    panic!("Code Mode exec tool is unavailable without the `code_mode` feature")
}

#[cfg(feature = "code_mode")]
pub fn is_code_mode_nested_tool(tool_name: &str) -> bool {
    praxis_code_mode::is_code_mode_nested_tool(tool_name)
}

#[cfg(not(feature = "code_mode"))]
pub fn is_code_mode_nested_tool(tool_name: &str) -> bool {
    tool_name != PUBLIC_TOOL_NAME && tool_name != WAIT_TOOL_NAME
}

#[cfg(feature = "code_mode")]
fn code_mode_tool_definition_for_spec(spec: &ToolSpec) -> Option<CodeModeToolDefinition> {
    match spec {
        ToolSpec::Function(tool) => Some(CodeModeToolDefinition {
            name: tool.name.clone(),
            description: tool.description.clone(),
            kind: CodeModeToolKind::Function,
            input_schema: serde_json::to_value(&tool.parameters).ok(),
            output_schema: tool.output_schema.clone(),
        }),
        ToolSpec::Freeform(tool) => Some(CodeModeToolDefinition {
            name: tool.name.clone(),
            description: tool.description.clone(),
            kind: CodeModeToolKind::Freeform,
            input_schema: None,
            output_schema: None,
        }),
        ToolSpec::LocalShell {}
        | ToolSpec::ImageGeneration { .. }
        | ToolSpec::ToolSearch { .. }
        | ToolSpec::WebSearch { .. } => None,
    }
}

#[cfg(all(test, feature = "code_mode"))]
#[path = "code_mode_tests.rs"]
mod tests;
