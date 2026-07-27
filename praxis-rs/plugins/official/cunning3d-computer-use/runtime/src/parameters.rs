use metra_computer_use::{
    ComputerUseAction, ComputerUseChannelKind, ComputerUseError, ComputerUseErrorKind,
    ComputerUseFallbackPolicy, ComputerUseRequest, ComputerUseResult, ComputerUseRoute,
    MAX_COMPUTER_USE_TIMEOUT_MS,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ComputerUseDynamicTool, INTERACT_TOOL_NAME, OBSERVE_TOOL_NAME, computer_use_dynamic_tool,
    is_computer_use_tool_namespace,
};

pub const MAX_DYNAMIC_TOOL_TIMEOUT_MS: u64 = MAX_COMPUTER_USE_TIMEOUT_MS;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDynamicToolArguments {
    action: Value,
    #[serde(default)]
    route: DynamicToolRouteArguments,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DynamicToolRouteArguments {
    #[serde(default)]
    preferred: Option<ComputerUseChannelKind>,
    #[serde(default)]
    allowed: Vec<ComputerUseChannelKind>,
    #[serde(default)]
    fallback: ComputerUseFallbackPolicy,
}

impl From<DynamicToolRouteArguments> for ComputerUseRoute {
    fn from(value: DynamicToolRouteArguments) -> Self {
        Self {
            preferred: value.preferred,
            allowed: value.allowed,
            fallback: value.fallback,
        }
    }
}

pub fn parse_dynamic_tool_call(
    tool_name: &str,
    arguments: Value,
) -> ComputerUseResult<ComputerUseRequest> {
    let Some(tool) = computer_use_dynamic_tool(tool_name) else {
        let message = if is_computer_use_tool_namespace(tool_name) {
            format!("unknown computer-use tool in plugin namespace: {tool_name}")
        } else {
            format!("tool is outside the computer-use namespace: {tool_name}")
        };
        return Err(ComputerUseError::new(
            ComputerUseErrorKind::Unsupported,
            message,
        ));
    };

    let raw: RawDynamicToolArguments = serde_json::from_value(arguments)
        .map_err(|error| invalid_input(format!("invalid arguments for {tool_name}: {error}")))?;
    validate_action_shape(&raw.action)?;
    let action: ComputerUseAction = serde_json::from_value(raw.action).map_err(|error| {
        invalid_input(format!(
            "invalid Metra computer-use action for {tool_name}: {error}"
        ))
    })?;

    let mut request = ComputerUseRequest::new(action);
    request.route = raw.route.into();
    if let Some(timeout_ms) = raw.timeout_ms {
        request.timeout_ms = timeout_ms;
    }
    request.validate()?;
    if !tool.accepts(&request.action) {
        return Err(invalid_input(format!(
            "action kind belongs to {}, not {tool_name}",
            match tool {
                ComputerUseDynamicTool::Observe => INTERACT_TOOL_NAME,
                ComputerUseDynamicTool::Interact => OBSERVE_TOOL_NAME,
            }
        )));
    }
    Ok(request)
}

fn validate_action_shape(action: &Value) -> ComputerUseResult<()> {
    let object = action
        .as_object()
        .ok_or_else(|| invalid_input("action must be a tagged JSON object"))?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_input("action.kind must be a string"))?;
    let Some((allowed, required)) = action_contract(kind) else {
        return Err(invalid_input(format!(
            "unsupported computer-use action kind: {kind}"
        )));
    };

    let unknown = object
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(invalid_input(format!(
            "unsupported fields for {kind}: {}",
            unknown.join(", ")
        ))
        .with_details(json!({
            "action_kind": kind,
            "unsupported_fields": unknown
        })));
    }

    let missing = required
        .iter()
        .filter(|field| !object.contains_key(**field))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(invalid_input(format!(
            "missing required fields for {kind}: {}",
            missing.join(", ")
        ))
        .with_details(json!({
            "action_kind": kind,
            "missing_fields": missing
        })));
    }
    Ok(())
}

fn action_contract(kind: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    match kind {
        "list_applications" | "list_windows" => Some((&["kind"], &["kind"])),
        "snapshot" => Some((
            &[
                "kind",
                "target",
                "detail",
                "max_depth",
                "max_nodes",
                "include_offscreen",
            ],
            &["kind"],
        )),
        "screenshot" => Some((&["kind", "target"], &["kind"])),
        "activate" | "toggle" | "select" | "focus" => {
            Some((&["kind", "target"], &["kind", "target"]))
        }
        "invoke" => Some((&["kind", "target", "external_effect"], &["kind", "target"])),
        "set_value" => Some((
            &["kind", "target", "value", "sensitive"],
            &["kind", "target", "value"],
        )),
        "click" => Some((
            &["kind", "target", "button", "count", "external_effect"],
            &["kind", "target"],
        )),
        "type_text" => Some((
            &["kind", "target", "text", "replace", "sensitive"],
            &["kind", "target", "text"],
        )),
        "key_input" => Some((&["kind", "target", "keys"], &["kind", "target", "keys"])),
        "scroll" => Some((
            &["kind", "target", "delta_x", "delta_y"],
            &["kind", "target", "delta_y"],
        )),
        "drag" => Some((
            &["kind", "from", "to", "duration_ms"],
            &["kind", "from", "to"],
        )),
        _ => None,
    }
}

fn invalid_input(message: impl Into<String>) -> ComputerUseError {
    ComputerUseError::new(ComputerUseErrorKind::InvalidInput, message)
}
