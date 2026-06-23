use super::*;

#[test]
fn mcp_server_elicitation_response_round_trips_rmcp_result() {
    let rmcp_result = rmcp::model::CreateElicitationResult {
        action: rmcp::model::ElicitationAction::Accept,
        content: Some(json!({
            "confirmed": true,
        })),
    };

    let api_response = McpServerElicitationRequestResponse::from(rmcp_result.clone());
    assert_eq!(
        api_response,
        McpServerElicitationRequestResponse {
            action: McpServerElicitationAction::Accept,
            content: Some(json!({
                "confirmed": true,
            })),
            meta: None,
        }
    );
    assert_eq!(
        rmcp::model::CreateElicitationResult::from(api_response),
        rmcp_result
    );
}

#[test]
fn mcp_server_elicitation_request_from_core_url_request() {
    let request = McpServerElicitationRequest::try_from(CoreElicitationRequest::Url {
        meta: None,
        message: "Finish sign-in".to_string(),
        url: "https://example.com/complete".to_string(),
        elicitation_id: "elicitation-123".to_string(),
    })
    .expect("URL request should convert");

    assert_eq!(
        request,
        McpServerElicitationRequest::Url {
            meta: None,
            message: "Finish sign-in".to_string(),
            url: "https://example.com/complete".to_string(),
            elicitation_id: "elicitation-123".to_string(),
        }
    );
}

#[test]
fn mcp_server_elicitation_request_from_core_form_request() {
    let request = McpServerElicitationRequest::try_from(CoreElicitationRequest::Form {
        meta: None,
        message: "Allow this request?".to_string(),
        requested_schema: json!({
            "type": "object",
            "properties": {
                "confirmed": {
                    "type": "boolean",
                }
            },
            "required": ["confirmed"],
        }),
    })
    .expect("form request should convert");

    let expected_schema: McpElicitationSchema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            "confirmed": {
                "type": "boolean",
            }
        },
        "required": ["confirmed"],
    }))
    .expect("expected schema should deserialize");

    assert_eq!(
        request,
        McpServerElicitationRequest::Form {
            meta: None,
            message: "Allow this request?".to_string(),
            requested_schema: expected_schema,
        }
    );
}

#[test]
fn mcp_elicitation_schema_matches_mcp_2025_11_25_primitives() {
    let schema: McpElicitationSchema = serde_json::from_value(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "email": {
                "type": "string",
                "title": "Email",
                "description": "Work email address",
                "format": "email",
                "default": "dev@example.com",
            },
            "count": {
                "type": "integer",
                "title": "Count",
                "description": "How many items to create",
                "minimum": 1,
                "maximum": 5,
                "default": 3,
            },
            "confirmed": {
                "type": "boolean",
                "title": "Confirm",
                "description": "Approve the pending action",
                "default": true,
            },
            "legacyChoice": {
                "type": "string",
                "title": "Action",
                "description": "Legacy titled enum form",
                "enum": ["allow", "deny"],
                "enumNames": ["Allow", "Deny"],
                "default": "allow",
            },
        },
        "required": ["email", "confirmed"],
    }))
    .expect("schema should deserialize");

    assert_eq!(
        schema,
        McpElicitationSchema {
            schema_uri: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            type_: McpElicitationObjectType::Object,
            properties: BTreeMap::from([
                (
                    "confirmed".to_string(),
                    McpElicitationPrimitiveSchema::Boolean(McpElicitationBooleanSchema {
                        type_: McpElicitationBooleanType::Boolean,
                        title: Some("Confirm".to_string()),
                        description: Some("Approve the pending action".to_string()),
                        default: Some(true),
                    }),
                ),
                (
                    "count".to_string(),
                    McpElicitationPrimitiveSchema::Number(McpElicitationNumberSchema {
                        type_: McpElicitationNumberType::Integer,
                        title: Some("Count".to_string()),
                        description: Some("How many items to create".to_string()),
                        minimum: Some(1.0),
                        maximum: Some(5.0),
                        default: Some(3.0),
                    }),
                ),
                (
                    "email".to_string(),
                    McpElicitationPrimitiveSchema::String(McpElicitationStringSchema {
                        type_: McpElicitationStringType::String,
                        title: Some("Email".to_string()),
                        description: Some("Work email address".to_string()),
                        min_length: None,
                        max_length: None,
                        format: Some(McpElicitationStringFormat::Email),
                        default: Some("dev@example.com".to_string()),
                    }),
                ),
                (
                    "legacyChoice".to_string(),
                    McpElicitationPrimitiveSchema::Enum(McpElicitationEnumSchema::Legacy(
                        McpElicitationLegacyTitledEnumSchema {
                            type_: McpElicitationStringType::String,
                            title: Some("Action".to_string()),
                            description: Some("Legacy titled enum form".to_string()),
                            enum_: vec!["allow".to_string(), "deny".to_string()],
                            enum_names: Some(vec!["Allow".to_string(), "Deny".to_string(),]),
                            default: Some("allow".to_string()),
                        },
                    )),
                ),
            ]),
            required: Some(vec!["email".to_string(), "confirmed".to_string()]),
        }
    );
}

#[test]
fn mcp_server_elicitation_request_rejects_null_core_form_schema() {
    let result = McpServerElicitationRequest::try_from(CoreElicitationRequest::Form {
        meta: Some(json!({
            "persist": "session",
        })),
        message: "Allow this request?".to_string(),
        requested_schema: JsonValue::Null,
    });

    assert!(result.is_err());
}

#[test]
fn mcp_server_elicitation_request_rejects_invalid_core_form_schema() {
    let result = McpServerElicitationRequest::try_from(CoreElicitationRequest::Form {
        meta: None,
        message: "Allow this request?".to_string(),
        requested_schema: json!({
            "type": "object",
            "properties": {
                "confirmed": {
                    "type": "object",
                }
            },
        }),
    });

    assert!(result.is_err());
}

#[test]
fn mcp_server_elicitation_response_serializes_nullable_content() {
    let response = McpServerElicitationRequestResponse {
        action: McpServerElicitationAction::Decline,
        content: None,
        meta: None,
    };

    assert_eq!(
        serde_json::to_value(response).expect("response should serialize"),
        json!({
            "action": "decline",
            "content": null,
            "_meta": null,
        })
    );
}
