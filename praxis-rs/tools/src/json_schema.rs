use serde::Deserialize;
use serde::Serialize;
use serde::ser::Error as _;
use serde_json::Map as JsonMap;
use serde_json::Value as JsonValue;
use serde_json::json;
use std::collections::BTreeMap;

/// Typed fast path plus a lossless raw representation for provider tool schemas.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JsonSchema {
    Boolean {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// MCP schema allows "number" | "integer" for Number.
    #[serde(alias = "integer")]
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Array {
        items: Box<JsonSchema>,

        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Object {
        properties: BTreeMap<String, JsonSchema>,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<Vec<String>>,
        #[serde(
            rename = "additionalProperties",
            skip_serializing_if = "Option::is_none"
        )]
        additional_properties: Option<AdditionalProperties>,
    },
    /// Lossless schema representation for constraints outside the typed subset.
    #[serde(skip_deserializing)]
    Raw { schema: JsonValue },
}

impl Serialize for JsonSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Self::Raw { schema } = self {
            return schema.serialize(serializer);
        }

        let mut schema = JsonMap::new();
        match self {
            Self::Boolean { description } => {
                schema.insert("type".to_string(), JsonValue::String("boolean".to_string()));
                insert_description(&mut schema, description);
            }
            Self::String { description } => {
                schema.insert("type".to_string(), JsonValue::String("string".to_string()));
                insert_description(&mut schema, description);
            }
            Self::Number { description } => {
                schema.insert("type".to_string(), JsonValue::String("number".to_string()));
                insert_description(&mut schema, description);
            }
            Self::Array { items, description } => {
                schema.insert("type".to_string(), JsonValue::String("array".to_string()));
                schema.insert(
                    "items".to_string(),
                    serde_json::to_value(items).map_err(S::Error::custom)?,
                );
                insert_description(&mut schema, description);
            }
            Self::Object {
                properties,
                required,
                additional_properties,
            } => {
                schema.insert("type".to_string(), JsonValue::String("object".to_string()));
                schema.insert(
                    "properties".to_string(),
                    serde_json::to_value(properties).map_err(S::Error::custom)?,
                );
                if let Some(required) = required {
                    schema.insert(
                        "required".to_string(),
                        serde_json::to_value(required).map_err(S::Error::custom)?,
                    );
                }
                if let Some(additional_properties) = additional_properties {
                    schema.insert(
                        "additionalProperties".to_string(),
                        serde_json::to_value(additional_properties).map_err(S::Error::custom)?,
                    );
                }
            }
            Self::Raw { .. } => unreachable!("raw schemas return before typed serialization"),
        }
        JsonValue::Object(schema).serialize(serializer)
    }
}

fn insert_description(schema: &mut JsonMap<String, JsonValue>, description: &Option<String>) {
    if let Some(description) = description {
        schema.insert(
            "description".to_string(),
            JsonValue::String(description.clone()),
        );
    }
}

/// Additional-properties policy with an optional nested schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<JsonSchema>),
}

impl From<bool> for AdditionalProperties {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<JsonSchema> for AdditionalProperties {
    fn from(value: JsonSchema) -> Self {
        Self::Schema(Box::new(value))
    }
}

/// Parse and normalize a tool schema while preserving unsupported constraints losslessly.
pub fn parse_tool_input_schema(input_schema: &JsonValue) -> Result<JsonSchema, serde_json::Error> {
    let mut input_schema = input_schema.clone();
    sanitize_json_schema(&mut input_schema);
    validate_json_schema(&input_schema)?;

    match serde_json::from_value::<JsonSchema>(input_schema.clone()) {
        Ok(parsed) => {
            let mut normalized_for_typed_comparison = input_schema.clone();
            normalize_integer_types(&mut normalized_for_typed_comparison);
            let typed_value = serde_json::to_value(&parsed)?;
            if typed_value == normalized_for_typed_comparison {
                Ok(parsed)
            } else {
                Ok(JsonSchema::Raw {
                    schema: input_schema,
                })
            }
        }
        Err(_) => Ok(JsonSchema::Raw {
            schema: input_schema,
        }),
    }
}

/// Normalize underspecified schemas without erasing references or composition constraints.
fn sanitize_json_schema(value: &mut JsonValue) {
    match value {
        JsonValue::Bool(_) => {
            // JSON Schema boolean form: true/false. Coerce to an accept-all string.
            *value = json!({ "type": "string" });
        }
        JsonValue::Array(values) => {
            for value in values {
                sanitize_json_schema(value);
            }
        }
        JsonValue::Object(map) => {
            if let Some(properties) = map.get_mut("properties")
                && let Some(properties_map) = properties.as_object_mut()
            {
                for value in properties_map.values_mut() {
                    sanitize_json_schema(value);
                }
            }
            for keyword in [
                "$defs",
                "definitions",
                "dependentSchemas",
                "patternProperties",
            ] {
                if let Some(schemas) = map.get_mut(keyword)
                    && let Some(schemas) = schemas.as_object_mut()
                {
                    for schema in schemas.values_mut() {
                        sanitize_json_schema(schema);
                    }
                }
            }
            if let Some(items) = map.get_mut("items") {
                sanitize_json_schema(items);
            }
            for combiner in ["oneOf", "anyOf", "allOf", "prefixItems"] {
                if let Some(value) = map.get_mut(combiner) {
                    sanitize_json_schema(value);
                }
            }
            for keyword in [
                "additionalProperties",
                "unevaluatedProperties",
                "propertyNames",
                "contains",
                "not",
                "if",
                "then",
                "else",
            ] {
                if let Some(schema) = map.get_mut(keyword)
                    && !schema.is_boolean()
                {
                    sanitize_json_schema(schema);
                }
            }

            let mut schema_type = map
                .get("type")
                .and_then(|value| value.as_str())
                .map(str::to_string);

            if schema_type.is_none()
                && let Some(JsonValue::Array(types)) = map.get("type")
            {
                for candidate in types {
                    if let Some(candidate_type) = candidate.as_str()
                        && matches!(
                            candidate_type,
                            "object" | "array" | "string" | "number" | "integer" | "boolean"
                        )
                    {
                        schema_type = Some(candidate_type.to_string());
                        break;
                    }
                }
            }

            if schema_type.is_none() {
                if map.contains_key("properties")
                    || map.contains_key("required")
                    || map.contains_key("additionalProperties")
                {
                    schema_type = Some("object".to_string());
                } else if map.contains_key("items") || map.contains_key("prefixItems") {
                    schema_type = Some("array".to_string());
                } else if map.contains_key("enum")
                    || map.contains_key("const")
                    || map.contains_key("format")
                {
                    schema_type = Some("string".to_string());
                } else if map.contains_key("minimum")
                    || map.contains_key("maximum")
                    || map.contains_key("exclusiveMinimum")
                    || map.contains_key("exclusiveMaximum")
                    || map.contains_key("multipleOf")
                {
                    schema_type = Some("number".to_string());
                }
            }

            let is_reference_or_composition = map.contains_key("$ref")
                || ["oneOf", "anyOf", "allOf", "not", "if"]
                    .iter()
                    .any(|keyword| map.contains_key(*keyword));
            let schema_type = schema_type
                .or_else(|| (!is_reference_or_composition).then(|| "string".to_string()));
            if let Some(schema_type) = &schema_type {
                map.insert("type".to_string(), JsonValue::String(schema_type.clone()));
            }

            if schema_type.as_deref() == Some("object") {
                if !map.contains_key("properties") && !is_reference_or_composition {
                    map.insert(
                        "properties".to_string(),
                        JsonValue::Object(serde_json::Map::new()),
                    );
                }
                if let Some(additional_properties) = map.get_mut("additionalProperties")
                    && !matches!(additional_properties, JsonValue::Bool(_))
                {
                    sanitize_json_schema(additional_properties);
                }
            }

            if schema_type.as_deref() == Some("array")
                && !map.contains_key("items")
                && !map.contains_key("prefixItems")
            {
                map.insert("items".to_string(), json!({ "type": "string" }));
            }
        }
        _ => {}
    }
}

fn normalize_integer_types(value: &mut JsonValue) {
    match value {
        JsonValue::Array(values) => {
            for value in values {
                normalize_integer_types(value);
            }
        }
        JsonValue::Object(map) => {
            if map.get("type").and_then(JsonValue::as_str) == Some("integer") {
                map.insert("type".to_string(), JsonValue::String("number".to_string()));
            }
            for value in map.values_mut() {
                normalize_integer_types(value);
            }
        }
        _ => {}
    }
}

fn validate_json_schema(value: &JsonValue) -> Result<(), serde_json::Error> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_schema("tool input schema must be an object after sanitization"))?;

    if let Some(reference) = object.get("$ref")
        && !reference.is_string()
    {
        return Err(invalid_schema("JSON Schema $ref must be a string"));
    }
    if let Some(description) = object.get("description")
        && !description.is_string()
    {
        return Err(invalid_schema("JSON Schema description must be a string"));
    }
    if let Some(required) = object.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| invalid_schema("JSON Schema required must be an array"))?;
        if required.iter().any(|name| !name.is_string()) {
            return Err(invalid_schema(
                "JSON Schema required entries must be strings",
            ));
        }
    }

    if let Some(schema_type) = object.get("type") {
        let Some(schema_type) = schema_type.as_str() else {
            return Err(invalid_schema("JSON Schema type must be a string"));
        };
        if !matches!(
            schema_type,
            "object" | "array" | "string" | "number" | "integer" | "boolean"
        ) {
            return Err(invalid_schema(format!(
                "unsupported JSON Schema type: {schema_type}"
            )));
        }
    } else if !object.contains_key("$ref")
        && !["oneOf", "anyOf", "allOf", "not", "if"]
            .iter()
            .any(|keyword| object.contains_key(*keyword))
    {
        return Err(invalid_schema(
            "JSON Schema requires a supported type, $ref, or composition keyword",
        ));
    }

    if let Some(properties) = object.get("properties") {
        let properties = properties
            .as_object()
            .ok_or_else(|| invalid_schema("JSON Schema properties must be an object"))?;
        for property in properties.values() {
            validate_json_schema(property)?;
        }
    }

    for keyword in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "patternProperties",
    ] {
        if let Some(schemas) = object.get(keyword) {
            let schemas = schemas.as_object().ok_or_else(|| {
                invalid_schema(format!("JSON Schema {keyword} must be an object"))
            })?;
            for schema in schemas.values() {
                validate_json_schema(schema)?;
            }
        }
    }

    if let Some(items) = object.get("items") {
        validate_json_schema(items)?;
    }
    for keyword in ["oneOf", "anyOf", "allOf", "prefixItems"] {
        if let Some(schemas) = object.get(keyword) {
            let schemas = schemas
                .as_array()
                .ok_or_else(|| invalid_schema(format!("JSON Schema {keyword} must be an array")))?;
            if schemas.is_empty() {
                return Err(invalid_schema(format!(
                    "JSON Schema {keyword} must not be empty"
                )));
            }
            for schema in schemas {
                validate_json_schema(schema)?;
            }
        }
    }
    for keyword in [
        "additionalProperties",
        "unevaluatedProperties",
        "propertyNames",
        "contains",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(schema) = object.get(keyword)
            && !schema.is_boolean()
        {
            validate_json_schema(schema)?;
        }
    }
    Ok(())
}

fn invalid_schema(message: impl Into<String>) -> serde_json::Error {
    <serde_json::Error as serde::de::Error>::custom(message.into())
}

#[cfg(test)]
#[path = "json_schema_tests.rs"]
mod tests;
