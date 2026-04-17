use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

pub fn create_team_read_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "include_teammates".to_string(),
            JsonSchema::Boolean {
                description: Some("When true, include teammate records in the result.".to_string()),
            },
        ),
        (
            "include_tasks".to_string(),
            JsonSchema::Boolean {
                description: Some("When true, include team tasks in the result.".to_string()),
            },
        ),
        (
            "include_messages".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "When true, include recent mailbox messages in the result.".to_string(),
                ),
            },
        ),
        (
            "message_limit".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Optional cap on the number of recent mailbox messages to return.".to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "team_read".to_string(),
        description: "Read the current thread's team context, including teammates, tasks, and recent mailbox messages. The current thread must be the team lead or a registered teammate.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
        output_schema: Some(team_read_output_schema()),
    })
}

pub fn create_team_send_message_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "recipient".to_string(),
            JsonSchema::String {
                description: Some(
                    "Mailbox recipient. Use `lead` for the team lead, or a teammate id for a teammate."
                        .to_string(),
                ),
            },
        ),
        (
            "body".to_string(),
            JsonSchema::String {
                description: Some("Mailbox message text.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "team_send_message".to_string(),
        description: "Append a durable mailbox message from the current team participant to the lead or a teammate. When the target thread is live, this also attempts immediate in-memory delivery and may wake the recipient.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["recipient".to_string(), "body".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(json!({
            "type": "object",
            "required": ["message"],
            "additionalProperties": false,
            "properties": {
                "message": team_message_output_schema(),
                "live_delivery": {
                    "type": ["object", "null"],
                    "required": ["target_thread_id", "submission_id"],
                    "additionalProperties": false,
                    "properties": {
                        "target_thread_id": { "type": "string" },
                        "submission_id": { "type": "string" }
                    }
                }
            }
        })),
    })
}

pub fn create_team_task_create_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "title".to_string(),
            JsonSchema::String {
                description: Some("Short task title.".to_string()),
            },
        ),
        (
            "description".to_string(),
            JsonSchema::String {
                description: Some("Optional task details.".to_string()),
            },
        ),
        (
            "assignee_teammate_id".to_string(),
            JsonSchema::String {
                description: Some("Optional teammate id to assign the task to.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "team_task_create".to_string(),
        description: "Create a new task in the current team.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["title".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(json!({
            "type": "object",
            "required": ["task"],
            "additionalProperties": false,
            "properties": {
                "task": team_task_output_schema()
            }
        })),
    })
}

pub fn create_team_task_list_tool() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "team_task_list".to_string(),
        description: "List tasks in the current team.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties: BTreeMap::new(),
            required: None,
            additional_properties: Some(false.into()),
        },
        output_schema: Some(json!({
            "type": "object",
            "required": ["data"],
            "additionalProperties": false,
            "properties": {
                "data": {
                    "type": "array",
                    "items": team_task_output_schema()
                }
            }
        })),
    })
}

pub fn create_team_task_update_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "task_id".to_string(),
            JsonSchema::String {
                description: Some("Task id to update.".to_string()),
            },
        ),
        (
            "title".to_string(),
            JsonSchema::String {
                description: Some("Optional replacement title.".to_string()),
            },
        ),
        (
            "description".to_string(),
            JsonSchema::String {
                description: Some("Optional replacement description.".to_string()),
            },
        ),
        (
            "status".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional task status. Supported values: `pending`, `in_progress`, `blocked`, `completed`."
                        .to_string(),
                ),
            },
        ),
        (
            "assignee_teammate_id".to_string(),
            JsonSchema::String {
                description: Some("Optional teammate id to assign the task to.".to_string()),
            },
        ),
        (
            "clear_assignee".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "When true, clear any existing task assignee.".to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "team_task_update".to_string(),
        description: "Update an existing task in the current team.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["task_id".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(json!({
            "type": "object",
            "required": ["task"],
            "additionalProperties": false,
            "properties": {
                "task": team_task_output_schema()
            }
        })),
    })
}

fn team_read_output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["team", "current_participant", "teammates", "tasks", "messages"],
        "additionalProperties": false,
        "properties": {
            "team": {
                "type": "object",
                "required": ["team_id", "lead_thread_id", "name"],
                "additionalProperties": false,
                "properties": {
                    "team_id": { "type": "string" },
                    "lead_thread_id": { "type": "string" },
                    "name": { "type": "string" },
                    "objective": { "type": ["string", "null"] }
                }
            },
            "current_participant": participant_output_schema(),
            "teammates": {
                "type": "array",
                "items": teammate_output_schema()
            },
            "tasks": {
                "type": "array",
                "items": team_task_output_schema()
            },
            "messages": {
                "type": "array",
                "items": team_message_output_schema()
            }
        }
    })
}

fn participant_output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["kind"],
        "additionalProperties": false,
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["lead", "teammate"]
            },
            "teammate_id": { "type": ["string", "null"] },
            "name": { "type": ["string", "null"] }
        }
    })
}

fn teammate_output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["team_id", "teammate_id", "name", "status", "created_at", "updated_at"],
        "additionalProperties": false,
        "properties": {
            "team_id": { "type": "string" },
            "teammate_id": { "type": "string" },
            "name": { "type": "string" },
            "role": { "type": ["string", "null"] },
            "status": {
                "type": "string",
                "enum": ["pending", "active", "failed", "closed"]
            },
            "thread_id": { "type": ["string", "null"] },
            "last_error": { "type": ["string", "null"] },
            "created_at": { "type": "number" },
            "updated_at": { "type": "number" }
        }
    })
}

fn team_task_output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["team_id", "task_id", "title", "status", "created_at", "updated_at"],
        "additionalProperties": false,
        "properties": {
            "team_id": { "type": "string" },
            "task_id": { "type": "string" },
            "title": { "type": "string" },
            "description": { "type": ["string", "null"] },
            "status": {
                "type": "string",
                "enum": ["pending", "in_progress", "blocked", "completed"]
            },
            "assignee_teammate_id": { "type": ["string", "null"] },
            "created_at": { "type": "number" },
            "updated_at": { "type": "number" },
            "completed_at": { "type": ["number", "null"] }
        }
    })
}

fn team_message_output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["message_id", "team_id", "sender", "recipient", "body", "created_at"],
        "additionalProperties": false,
        "properties": {
            "message_id": { "type": "string" },
            "team_id": { "type": "string" },
            "sender": participant_output_schema(),
            "recipient": participant_output_schema(),
            "body": { "type": "string" },
            "created_at": { "type": "number" }
        }
    })
}
