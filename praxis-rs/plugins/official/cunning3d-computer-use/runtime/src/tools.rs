use metra_computer_use::{
    ComputerUseAction, ComputerUseRiskClass, MAX_CLICK_COUNT, MAX_COMPUTER_USE_TIMEOUT_MS,
    MAX_DRAG_DURATION_MS, MAX_KEY_MODIFIERS, MAX_KEY_NAME_CHARS, MAX_KEY_STROKES,
    MAX_SELECTOR_ATTRIBUTES, MAX_SELECTOR_BRANCH, MAX_SELECTOR_INDEX, MAX_SELECTOR_STRING_CHARS,
    MAX_SNAPSHOT_DEPTH, MAX_SNAPSHOT_NODES, MAX_TARGET_ID_CHARS, MAX_TEXT_INPUT_CHARS,
    MAX_WINDOW_TITLE_CHARS, MIN_COMPUTER_USE_TIMEOUT_MS,
};
use serde_json::{Map, Value, json};

pub const COMPUTER_USE_TOOL_NAMESPACE: &str = "cunning3d_computer_use";
pub const COMPUTER_USE_TOOL_PREFIX: &str = "cunning3d_computer_use_";
pub const OBSERVE_TOOL_NAME: &str = "cunning3d_computer_use_observe";
pub const INTERACT_TOOL_NAME: &str = "cunning3d_computer_use_interact";

const TARGET_REF: &str = "#/$defs/computer_use_target";
const SELECTOR_REF: &str = "#/$defs/element_selector";
const WINDOW_SELECTOR_REF: &str = "#/$defs/window_selector";
const SCREEN_POINT_REF: &str = "#/$defs/screen_point";
const KEY_STROKE_REF: &str = "#/$defs/key_stroke";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerUseDynamicTool {
    Observe,
    Interact,
}

impl ComputerUseDynamicTool {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Observe => OBSERVE_TOOL_NAME,
            Self::Interact => INTERACT_TOOL_NAME,
        }
    }

    pub const fn accepts(self, action: &ComputerUseAction) -> bool {
        match self {
            Self::Observe => matches!(action.risk(), ComputerUseRiskClass::Observe),
            Self::Interact => !matches!(action.risk(), ComputerUseRiskClass::Observe),
        }
    }
}

pub fn is_computer_use_tool_namespace(tool_name: &str) -> bool {
    tool_name == COMPUTER_USE_TOOL_NAMESPACE || tool_name.starts_with(COMPUTER_USE_TOOL_PREFIX)
}

pub fn computer_use_dynamic_tool(tool_name: &str) -> Option<ComputerUseDynamicTool> {
    match tool_name {
        OBSERVE_TOOL_NAME => Some(ComputerUseDynamicTool::Observe),
        INTERACT_TOOL_NAME => Some(ComputerUseDynamicTool::Interact),
        _ => None,
    }
}

pub fn recognizes_computer_use_tool(tool_name: &str) -> bool {
    computer_use_dynamic_tool(tool_name).is_some()
}

pub fn dynamic_tool_definitions() -> Vec<Value> {
    vec![
        dynamic_tool_definition(
            ComputerUseDynamicTool::Observe,
            "Observe native UI through Metra. Lists applications or windows, captures a semantic snapshot, or takes a screenshot.",
            observe_action_schema(),
        ),
        dynamic_tool_definition(
            ComputerUseDynamicTool::Interact,
            "Interact with native UI through Metra. Prefer stable semantic targets; runtime risk classification never trusts model-supplied intent metadata.",
            interact_action_schema(),
        ),
    ]
}

fn dynamic_tool_definition(
    tool: ComputerUseDynamicTool,
    description: &str,
    action_schema: Value,
) -> Value {
    json!({
        "name": tool.name(),
        "description": description,
        "inputSchema": {
            "type": "object",
            "$defs": schema_definitions(),
            "properties": {
                "action": action_schema,
                "route": route_schema(),
                "timeout_ms": {
                    "type": "integer",
                    "minimum": MIN_COMPUTER_USE_TIMEOUT_MS,
                    "maximum": MAX_COMPUTER_USE_TIMEOUT_MS,
                    "description": "Optional per-call timeout in milliseconds."
                }
            },
            "required": ["action"],
            "additionalProperties": false
        },
        "deferLoading": true
    })
}

fn schema_definitions() -> Value {
    json!({
        "computer_use_target": target_schema(),
        "element_selector": selector_schema(),
        "window_selector": window_selector_schema(),
        "screen_point": screen_point_schema(),
        "key_stroke": key_stroke_schema()
    })
}

fn observe_action_schema() -> Value {
    json!({
        "type": "object",
        "description": "Exactly one tagged observation action.",
        "oneOf": [
            tagged_variant("list_applications", json!({}), &[]),
            tagged_variant("list_windows", json!({}), &[]),
            tagged_variant(
                "snapshot",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "detail": {
                        "type": "string",
                        "enum": ["summary", "standard", "full"]
                    },
                    "max_depth": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_SNAPSHOT_DEPTH
                    },
                    "max_nodes": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_SNAPSHOT_NODES
                    },
                    "include_offscreen": {
                        "type": "boolean"
                    }
                }),
                &[],
            ),
            tagged_variant(
                "screenshot",
                json!({
                    "target": schema_ref(TARGET_REF)
                }),
                &[],
            )
        ]
    })
}

fn interact_action_schema() -> Value {
    json!({
        "type": "object",
        "description": "Exactly one tagged interaction action.",
        "oneOf": [
            target_action_variant("activate"),
            tagged_variant(
                "invoke",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "external_effect": {
                        "type": "boolean",
                        "description": "Intent metadata; invoke is conservatively classified regardless of this value."
                    }
                }),
                &["target"],
            ),
            tagged_variant(
                "set_value",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "value": bounded_string_schema(MAX_TEXT_INPUT_CHARS, false),
                    "sensitive": {
                        "type": "boolean",
                        "description": "Intent metadata; value input is always treated as sensitive."
                    }
                }),
                &["target", "value"],
            ),
            target_action_variant("toggle"),
            target_action_variant("select"),
            target_action_variant("focus"),
            tagged_variant(
                "click",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "button": {
                        "type": "string",
                        "enum": ["left", "right", "middle"]
                    },
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_CLICK_COUNT
                    },
                    "external_effect": {
                        "type": "boolean",
                        "description": "Intent metadata; clicks are conservatively classified regardless of this value."
                    }
                }),
                &["target"],
            ),
            tagged_variant(
                "type_text",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "text": bounded_string_schema(MAX_TEXT_INPUT_CHARS, false),
                    "replace": {
                        "type": "boolean"
                    },
                    "sensitive": {
                        "type": "boolean",
                        "description": "Intent metadata; text input is always treated as sensitive."
                    }
                }),
                &["target", "text"],
            ),
            tagged_variant(
                "key_input",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "keys": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_KEY_STROKES,
                        "items": schema_ref(KEY_STROKE_REF)
                    }
                }),
                &["target", "keys"],
            ),
            tagged_variant(
                "scroll",
                json!({
                    "target": schema_ref(TARGET_REF),
                    "delta_x": signed_32_schema(),
                    "delta_y": signed_32_schema()
                }),
                &["target", "delta_y"],
            ),
            tagged_variant(
                "drag",
                json!({
                    "from": schema_ref(SCREEN_POINT_REF),
                    "to": schema_ref(SCREEN_POINT_REF),
                    "duration_ms": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": MAX_DRAG_DURATION_MS
                    }
                }),
                &["from", "to"],
            )
        ]
    })
}

fn target_action_variant(kind: &str) -> Value {
    tagged_variant(
        kind,
        json!({
            "target": schema_ref(TARGET_REF)
        }),
        &["target"],
    )
}

fn target_schema() -> Value {
    json!({
        "type": "object",
        "description": "Exactly one tagged Metra target.",
        "oneOf": [
            tagged_variant("desktop", json!({}), &[]),
            tagged_variant(
                "product",
                json!({
                    "product_id": bounded_string_schema(MAX_TARGET_ID_CHARS, true),
                    "surface_id": bounded_string_schema(MAX_TARGET_ID_CHARS, true),
                    "selector": schema_ref(SELECTOR_REF)
                }),
                &["product_id"],
            ),
            tagged_variant(
                "window",
                json!({
                    "window": schema_ref(WINDOW_SELECTOR_REF)
                }),
                &["window"],
            ),
            tagged_variant(
                "element",
                json!({
                    "window": schema_ref(WINDOW_SELECTOR_REF),
                    "selector": schema_ref(SELECTOR_REF)
                }),
                &["selector"],
            ),
            tagged_variant(
                "point",
                json!({
                    "point": schema_ref(SCREEN_POINT_REF)
                }),
                &["point"],
            ),
            tagged_variant(
                "browser",
                json!({
                    "tab_id": bounded_string_schema(MAX_TARGET_ID_CHARS, true),
                    "selector": schema_ref(SELECTOR_REF)
                }),
                &[],
            )
        ]
    })
}

fn selector_schema() -> Value {
    let string_value = || bounded_string_schema(MAX_SELECTOR_STRING_CHARS, true);
    let match_fields = || {
        json!({
            "value": string_value(),
            "mode": {
                "type": "string",
                "enum": ["exact", "contains", "prefix", "suffix"]
            },
            "case_sensitive": {
                "type": "boolean"
            }
        })
    };
    json!({
        "type": "object",
        "description": "Exactly one tagged Metra element selector.",
        "oneOf": [
            tagged_variant(
                "handle",
                json!({
                    "snapshot_id": string_value(),
                    "node_id": string_value()
                }),
                &["snapshot_id", "node_id"],
            ),
            tagged_variant(
                "automation_id",
                json!({
                    "value": string_value()
                }),
                &["value"],
            ),
            tagged_variant(
                "native_id",
                json!({
                    "value": string_value()
                }),
                &["value"],
            ),
            tagged_variant(
                "role",
                json!({
                    "role": {
                        "type": "string",
                        "enum": element_roles()
                    }
                }),
                &["role"],
            ),
            tagged_variant("name", match_fields(), &["value"]),
            tagged_variant("text", match_fields(), &["value"]),
            tagged_variant(
                "attributes",
                json!({
                    "values": {
                        "type": "object",
                        "minProperties": 1,
                        "maxProperties": MAX_SELECTOR_ATTRIBUTES,
                        "propertyNames": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": MAX_SELECTOR_STRING_CHARS,
                            "pattern": "\\S"
                        },
                        "additionalProperties": {
                            "type": "string",
                            "maxLength": MAX_SELECTOR_STRING_CHARS
                        }
                    }
                }),
                &["values"],
            ),
            tagged_variant(
                "point",
                json!({
                    "x": signed_32_schema(),
                    "y": signed_32_schema()
                }),
                &["x", "y"],
            ),
            tagged_variant(
                "and",
                json!({
                    "selectors": selector_array_schema()
                }),
                &["selectors"],
            ),
            tagged_variant(
                "or",
                json!({
                    "selectors": selector_array_schema()
                }),
                &["selectors"],
            ),
            tagged_variant(
                "not",
                json!({
                    "selector": schema_ref(SELECTOR_REF)
                }),
                &["selector"],
            ),
            tagged_variant(
                "descendant",
                json!({
                    "ancestor": schema_ref(SELECTOR_REF),
                    "target": schema_ref(SELECTOR_REF)
                }),
                &["ancestor", "target"],
            ),
            tagged_variant(
                "nth",
                json!({
                    "selector": schema_ref(SELECTOR_REF),
                    "index": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": MAX_SELECTOR_INDEX
                    }
                }),
                &["selector", "index"],
            )
        ]
    })
}

fn selector_array_schema() -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "maxItems": MAX_SELECTOR_BRANCH,
        "items": schema_ref(SELECTOR_REF)
    })
}

fn window_selector_schema() -> Value {
    json!({
        "type": "object",
        "description": "A non-empty window selector.",
        "properties": {
            "native_handle": unsigned_64_schema(),
            "process_id": {
                "type": "integer",
                "minimum": 1,
                "maximum": 4294967295_u64
            },
            "title": bounded_string_schema(MAX_WINDOW_TITLE_CHARS, true),
            "exact_title": {
                "type": "boolean"
            }
        },
        "anyOf": [
            {
                "required": ["native_handle"]
            },
            {
                "required": ["process_id"]
            },
            {
                "required": ["title"]
            }
        ],
        "allOf": [
            {
                "if": {
                    "properties": {
                        "exact_title": {
                            "const": true
                        }
                    },
                    "required": ["exact_title"]
                },
                "then": {
                    "required": ["title"]
                }
            }
        ],
        "additionalProperties": false
    })
}

fn screen_point_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "x": signed_32_schema(),
            "y": signed_32_schema(),
            "space": {
                "type": "string",
                "enum": [
                    "screen_physical",
                    "window_client",
                    "surface_logical"
                ]
            }
        },
        "required": ["x", "y"],
        "additionalProperties": false
    })
}

fn key_stroke_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "modifiers": {
                "type": "array",
                "maxItems": MAX_KEY_MODIFIERS,
                "items": bounded_string_schema(MAX_KEY_NAME_CHARS, true)
            },
            "key": bounded_string_schema(MAX_KEY_NAME_CHARS, true)
        },
        "required": ["key"],
        "additionalProperties": false
    })
}

fn route_schema() -> Value {
    let channel = || {
        json!({
            "type": "string",
            "enum": [
                "product_native",
                "browser_dom",
                "accessibility",
                "vision",
                "pointer"
            ]
        })
    };
    json!({
        "type": "object",
        "description": "Optional Metra channel routing policy.",
        "properties": {
            "preferred": channel(),
            "allowed": {
                "type": "array",
                "maxItems": 5,
                "uniqueItems": true,
                "items": channel()
            },
            "fallback": {
                "type": "string",
                "enum": ["disabled", "safe_only"]
            }
        },
        "additionalProperties": false
    })
}

fn tagged_variant(kind: &str, fields: Value, required: &[&str]) -> Value {
    let mut properties = fields.as_object().cloned().unwrap_or_default();
    properties.insert(
        "kind".to_owned(),
        json!({
            "type": "string",
            "const": kind
        }),
    );
    let required = std::iter::once("kind")
        .chain(required.iter().copied())
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": required,
        "additionalProperties": false
    })
}

fn bounded_string_schema(max_length: usize, require_non_empty: bool) -> Value {
    let mut schema = Map::new();
    schema.insert("type".to_owned(), Value::String("string".to_owned()));
    schema.insert("maxLength".to_owned(), json!(max_length));
    if require_non_empty {
        schema.insert("minLength".to_owned(), json!(1));
        schema.insert("pattern".to_owned(), Value::String("\\S".to_owned()));
    }
    Value::Object(schema)
}

fn schema_ref(reference: &str) -> Value {
    json!({ "$ref": reference })
}

fn signed_32_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": i32::MIN,
        "maximum": i32::MAX
    })
}

fn unsigned_64_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": u64::MAX
    })
}

fn element_roles() -> Vec<&'static str> {
    vec![
        "root",
        "application",
        "window",
        "pane",
        "container",
        "button",
        "check_box",
        "radio_button",
        "input",
        "text",
        "label",
        "link",
        "image",
        "list",
        "list_item",
        "combo_box",
        "menu",
        "menu_item",
        "tab",
        "tab_item",
        "tree",
        "tree_item",
        "scroll_bar",
        "document",
        "canvas",
        "custom",
        "unknown",
    ]
}
