use super::*;

pub(super) fn parse_fields_from_schema(
    requested_schema: &Value,
) -> Option<Vec<McpServerElicitationField>> {
    let schema = requested_schema.as_object()?;
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return None;
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<HashSet<_>>();
    let properties = schema.get("properties")?.as_object()?;
    let mut fields = Vec::new();
    for (id, property_schema) in properties {
        let property =
            serde_json::from_value::<McpElicitationPrimitiveSchema>(property_schema.clone())
                .ok()?;
        fields.push(parse_field(id, property, required.contains(id))?);
    }
    if fields.is_empty() {
        return None;
    }
    Some(fields)
}

fn parse_field(
    id: &str,
    property: McpElicitationPrimitiveSchema,
    required: bool,
) -> Option<McpServerElicitationField> {
    match property {
        McpElicitationPrimitiveSchema::String(schema) => {
            let label = schema.title.unwrap_or_else(|| id.to_string());
            let prompt = schema.description.unwrap_or_else(|| label.clone());
            Some(McpServerElicitationField {
                id: id.to_string(),
                label,
                prompt,
                required,
                input: McpServerElicitationFieldInput::Text { secret: false },
            })
        }
        McpElicitationPrimitiveSchema::Boolean(schema) => {
            let label = schema.title.unwrap_or_else(|| id.to_string());
            let prompt = schema.description.unwrap_or_else(|| label.clone());
            let default_idx = schema.default.map(|value| if value { 0 } else { 1 });
            let options = [true, false]
                .into_iter()
                .map(|value| {
                    let label = if value { "True" } else { "False" }.to_string();
                    McpServerElicitationOption {
                        label,
                        description: None,
                        value: Value::Bool(value),
                    }
                })
                .collect();
            Some(McpServerElicitationField {
                id: id.to_string(),
                label,
                prompt,
                required,
                input: McpServerElicitationFieldInput::Select {
                    options,
                    default_idx,
                },
            })
        }
        McpElicitationPrimitiveSchema::Enum(McpElicitationEnumSchema::Legacy(schema)) => {
            let label = schema.title.unwrap_or_else(|| id.to_string());
            let prompt = schema.description.unwrap_or_else(|| label.clone());
            let default_idx = schema
                .default
                .as_ref()
                .and_then(|value| schema.enum_.iter().position(|entry| entry == value));
            let enum_names = schema.enum_names.unwrap_or_default();
            let options = schema
                .enum_
                .into_iter()
                .enumerate()
                .map(|(idx, value)| McpServerElicitationOption {
                    label: enum_names
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| value.clone()),
                    description: None,
                    value: Value::String(value),
                })
                .collect();
            Some(McpServerElicitationField {
                id: id.to_string(),
                label,
                prompt,
                required,
                input: McpServerElicitationFieldInput::Select {
                    options,
                    default_idx,
                },
            })
        }
        McpElicitationPrimitiveSchema::Enum(McpElicitationEnumSchema::SingleSelect(schema)) => {
            parse_single_select_field(id, schema, required)
        }
        McpElicitationPrimitiveSchema::Number(_)
        | McpElicitationPrimitiveSchema::Enum(McpElicitationEnumSchema::MultiSelect(_)) => None,
    }
}

fn parse_single_select_field(
    id: &str,
    schema: McpElicitationSingleSelectEnumSchema,
    required: bool,
) -> Option<McpServerElicitationField> {
    match schema {
        McpElicitationSingleSelectEnumSchema::Untitled(schema) => {
            let label = schema.title.unwrap_or_else(|| id.to_string());
            let prompt = schema.description.unwrap_or_else(|| label.clone());
            let default_idx = schema
                .default
                .as_ref()
                .and_then(|value| schema.enum_.iter().position(|entry| entry == value));
            let options = schema
                .enum_
                .into_iter()
                .map(|value| McpServerElicitationOption {
                    label: value.clone(),
                    description: None,
                    value: Value::String(value),
                })
                .collect();
            Some(McpServerElicitationField {
                id: id.to_string(),
                label,
                prompt,
                required,
                input: McpServerElicitationFieldInput::Select {
                    options,
                    default_idx,
                },
            })
        }
        McpElicitationSingleSelectEnumSchema::Titled(schema) => {
            let label = schema.title.unwrap_or_else(|| id.to_string());
            let prompt = schema.description.unwrap_or_else(|| label.clone());
            let default_idx = schema.default.as_ref().and_then(|value| {
                schema
                    .one_of
                    .iter()
                    .position(|entry| entry.const_.as_str() == value)
            });
            let options = schema
                .one_of
                .into_iter()
                .map(|entry| McpServerElicitationOption {
                    label: entry.title,
                    description: None,
                    value: Value::String(entry.const_),
                })
                .collect();
            Some(McpServerElicitationField {
                id: id.to_string(),
                label,
                prompt,
                required,
                input: McpServerElicitationFieldInput::Select {
                    options,
                    default_idx,
                },
            })
        }
    }
}
