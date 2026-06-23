use super::*;

impl McpServerElicitationFormRequest {
    pub(crate) fn from_app_gateway_request(
        thread_id: ThreadId,
        request_id: McpRequestId,
        request: McpServerElicitationRequestParams,
    ) -> Option<Self> {
        let McpServerElicitationRequestParams {
            server_name,
            request,
            ..
        } = request;
        let McpServerElicitationRequest::Form {
            meta,
            message,
            requested_schema,
        } = request
        else {
            return None;
        };

        let requested_schema = serde_json::to_value(requested_schema).ok()?;
        Self::from_parts(
            thread_id,
            server_name,
            request_id,
            meta,
            message,
            requested_schema,
        )
    }

    pub(crate) fn from_event(
        thread_id: ThreadId,
        request: ElicitationRequestEvent,
    ) -> Option<Self> {
        let ElicitationRequest::Form {
            meta,
            message,
            requested_schema,
        } = request.request
        else {
            return None;
        };

        Self::from_parts(
            thread_id,
            request.server_name,
            request.id,
            meta,
            message,
            requested_schema,
        )
    }

    fn from_parts(
        thread_id: ThreadId,
        server_name: String,
        request_id: McpRequestId,
        meta: Option<Value>,
        message: String,
        requested_schema: Value,
    ) -> Option<Self> {
        let tool_suggestion = parse_tool_suggestion_request(meta.as_ref());
        let is_tool_approval = meta
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|meta| meta.get(APPROVAL_META_KIND_KEY))
            .and_then(Value::as_str)
            == Some(APPROVAL_META_KIND_MCP_TOOL_CALL);
        let is_empty_object_schema = requested_schema.as_object().is_some_and(|schema| {
            schema.get("type").and_then(Value::as_str) == Some("object")
                && schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(serde_json::Map::is_empty)
        });
        let is_tool_approval_action =
            is_tool_approval && (requested_schema.is_null() || is_empty_object_schema);
        let approval_display_params = if is_tool_approval_action {
            parse_tool_approval_display_params(meta.as_ref())
        } else {
            Vec::new()
        };

        let (response_mode, fields) = if tool_suggestion.is_some()
            && (requested_schema.is_null() || is_empty_object_schema)
        {
            (McpServerElicitationResponseMode::FormContent, Vec::new())
        } else if requested_schema.is_null() || (is_tool_approval && is_empty_object_schema) {
            let mut options = vec![McpServerElicitationOption {
                label: "Allow".to_string(),
                description: Some("Run the tool and continue.".to_string()),
                value: Value::String(APPROVAL_ACCEPT_ONCE_VALUE.to_string()),
            }];
            if is_tool_approval_action
                && tool_approval_supports_persist_mode(
                    meta.as_ref(),
                    APPROVAL_PERSIST_SESSION_VALUE,
                )
            {
                options.push(McpServerElicitationOption {
                    label: "Allow for this session".to_string(),
                    description: Some(
                        "Run the tool and remember this choice for this session.".to_string(),
                    ),
                    value: Value::String(APPROVAL_ACCEPT_SESSION_VALUE.to_string()),
                });
            }
            if is_tool_approval_action
                && tool_approval_supports_persist_mode(meta.as_ref(), APPROVAL_PERSIST_ALWAYS_VALUE)
            {
                options.push(McpServerElicitationOption {
                    label: "Always allow".to_string(),
                    description: Some(
                        "Run the tool and remember this choice for future tool calls.".to_string(),
                    ),
                    value: Value::String(APPROVAL_ACCEPT_ALWAYS_VALUE.to_string()),
                });
            }
            if is_tool_approval_action {
                options.push(McpServerElicitationOption {
                    label: "Cancel".to_string(),
                    description: Some("Cancel this tool call".to_string()),
                    value: Value::String(APPROVAL_CANCEL_VALUE.to_string()),
                });
            } else {
                options.extend([
                    McpServerElicitationOption {
                        label: "Deny".to_string(),
                        description: Some("Decline this tool call and continue.".to_string()),
                        value: Value::String(APPROVAL_DECLINE_VALUE.to_string()),
                    },
                    McpServerElicitationOption {
                        label: "Cancel".to_string(),
                        description: Some("Cancel this tool call".to_string()),
                        value: Value::String(APPROVAL_CANCEL_VALUE.to_string()),
                    },
                ]);
            }
            (
                McpServerElicitationResponseMode::ApprovalAction,
                vec![McpServerElicitationField {
                    id: APPROVAL_FIELD_ID.to_string(),
                    label: String::new(),
                    prompt: String::new(),
                    required: true,
                    input: McpServerElicitationFieldInput::Select {
                        options,
                        default_idx: Some(0),
                    },
                }],
            )
        } else {
            (
                McpServerElicitationResponseMode::FormContent,
                parse_fields_from_schema(&requested_schema)?,
            )
        };

        Some(Self {
            thread_id,
            server_name,
            request_id,
            message,
            approval_display_params,
            response_mode,
            fields,
            tool_suggestion,
        })
    }

    pub(crate) fn tool_suggestion(&self) -> Option<&ToolSuggestionRequest> {
        self.tool_suggestion.as_ref()
    }

    pub(crate) fn thread_id(&self) -> ThreadId {
        self.thread_id
    }

    pub(crate) fn server_name(&self) -> &str {
        self.server_name.as_str()
    }

    pub(crate) fn request_id(&self) -> &McpRequestId {
        &self.request_id
    }
}
