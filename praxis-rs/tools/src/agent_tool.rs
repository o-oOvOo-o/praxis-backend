use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use praxis_protocol::openai_models::ModelPreset;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SpawnAgentToolOptions<'a> {
    pub available_models: &'a [ModelPreset],
    pub agent_type_description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitAgentTimeoutOptions {
    pub default_timeout_ms: i64,
    pub min_timeout_ms: i64,
    pub max_timeout_ms: i64,
}

pub fn create_spawn_agent_tool(options: SpawnAgentToolOptions<'_>) -> ToolSpec {
    let available_models_description = spawn_agent_models_description(options.available_models);
    let return_value_description = "Returns the canonical task name plus Praxis agent identity fields: base name, title, and display name.";
    let mut properties = spawn_agent_common_properties(&options.agent_type_description);
    properties.insert(
        "task_name".to_string(),
        JsonSchema::String {
            description: Some(
                "Canonical task name for the new agent. Use lowercase letters, digits, and underscores; this is the stable tool reference, not the UI label."
                    .to_string(),
            ),
        },
    );
    properties.insert(
        "title".to_string(),
        JsonSchema::String {
            description: Some(
                "Optional short human-facing responsibility label for the new agent, such as `负责GUI` or `碰撞系统`. Praxis combines it with a Chinese base name, for example `墨子-负责GUI`; when omitted, Praxis derives a label from `task_name`."
                    .to_string(),
            ),
        },
    );

    ToolSpec::Function(ResponsesApiTool {
        name: "spawn_agent".to_string(),
        description: spawn_agent_tool_description(
            &available_models_description,
            return_value_description,
        ),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["task_name".to_string(), "message".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(spawn_agent_output_schema()),
    })
}

pub fn create_send_message_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        ("target".to_string(), agent_target_schema("message")),
        (
            "message".to_string(),
            JsonSchema::String {
                description: Some(
                    "Message text to queue on the target agent. This does not wake the target or produce a new result by itself; use assign_task when you need the target to run now.".to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "send_message".to_string(),
        description: "Queue a text message for an existing agent without triggering a new turn. Do not call wait_agent expecting send_message to produce a fresh target reply; use assign_task for work that must run now and return a result.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["target".to_string(), "message".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(message_submission_output_schema()),
    })
}

pub fn create_assign_task_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        ("target".to_string(), agent_target_schema("assign")),
        (
            "objective".to_string(),
            JsonSchema::String {
                description: Some("Concrete task objective owned by AgentOS.".to_string()),
            },
        ),
        (
            "message".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional brief worker prompt. Defaults to objective when omitted."
                        .to_string(),
                ),
            },
        ),
        (
            "scope".to_string(),
            string_array_schema(
                "Files, modules, or logical areas this task may touch. Required unless exploratory is true.",
            ),
        ),
        (
            "constraints".to_string(),
            string_array_schema("Hard constraints the worker must obey."),
        ),
        (
            "acceptance_criteria".to_string(),
            string_array_schema("Concrete completion checks for the assigned task."),
        ),
        (
            "artifact_refs".to_string(),
            string_array_schema("Artifact ids or URIs the worker should inspect."),
        ),
        (
            "required_capabilities".to_string(),
            string_array_schema("Capability names required by this task."),
        ),
        (
            "required_resources".to_string(),
            string_array_schema(
                "Resource leases requested by this task, such as cpu_heavy, build_cache:praxis, repo_write:tui/src/**, port:3000, gpu:0, network:default, or llm_budget:task.",
            ),
        ),
        (
            "token_budget".to_string(),
            JsonSchema::Number {
                description: Some("Optional task token budget.".to_string()),
            },
        ),
        (
            "priority".to_string(),
            JsonSchema::Number {
                description: Some("Optional scheduler priority for this task.".to_string()),
            },
        ),
        (
            "exploratory".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "When true, permits an initially empty scope for discovery tasks."
                        .to_string(),
                ),
            },
        ),
        (
            "interrupt".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "When true, stop the agent's current task and handle this immediately. When false (default), queue this message."
                        .to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "assign_task".to_string(),
        description: "Create a structured AgentOS task for an existing non-root agent and trigger a new turn in the target. Use this, not send_message, when the target must do new work and return a result. Scope and resources become runtime scheduling facts, not chat-only guidance.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec![
                "target".to_string(),
                "objective".to_string(),
                "scope".to_string(),
            ]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(message_submission_output_schema()),
    })
}

fn string_array_schema(description: &str) -> JsonSchema {
    JsonSchema::Array {
        items: Box::new(JsonSchema::String { description: None }),
        description: Some(description.to_string()),
    }
}

fn agent_target_schema(action: &str) -> JsonSchema {
    JsonSchema::String {
        description: Some(format!(
            "Stable target for the agent to {action}. Prefer `recommended_target` from spawn_agent or list_agents; it is normally the thread id and is the most reliable value. Do not use `agent_name`, canonical task names, display names, or Chinese base names when `recommended_target` is available; those are accepted only for recovery or older tool outputs."
        )),
    }
}

pub fn create_wait_agent_tool(options: WaitAgentTimeoutOptions) -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "wait_agent".to_string(),
        description: "Wait for an agent update. With target, wait for that target agent to reach a final status and return the target status. Without target, wait for any mailbox or AgentOS state update. wait_agent never wakes an idle target by itself."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: wait_agent_tool_parameters(options),
        output_schema: Some(wait_output_schema()),
    })
}

pub fn create_list_agents_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "path_prefix".to_string(),
        JsonSchema::String {
            description: Some(
                "Optional task-path prefix. Accepts the same relative or absolute task-path syntax as canonical agent targets."
                    .to_string(),
            ),
        },
    )]);

    ToolSpec::Function(ResponsesApiTool {
        name: "list_agents".to_string(),
        description:
            "List live sub-agents in the current root thread tree. Optionally filter by task-path prefix. The current `/root` main agent is intentionally omitted. If `agents` is empty and AgentOS pending lists are empty, all sub-agents are closed or absent, so stop listing and summarize."
                .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
        output_schema: Some(list_agents_output_schema()),
    })
}

pub fn create_read_agent_artifact_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "artifact_id".to_string(),
            JsonSchema::String {
                description: Some(
                    "AgentOS artifact id returned by list_agents or another AgentOS tool."
                        .to_string(),
                ),
            },
        ),
        (
            "max_bytes".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Optional byte limit for the blob read. The runtime clamps this to a safe maximum."
                        .to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "read_agent_artifact".to_string(),
        description: "Read a bounded slice of an AgentOS artifact blob by id. Use this instead of asking workers to paste long logs in chat.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["artifact_id".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(read_agent_artifact_output_schema()),
    })
}

pub fn create_submit_worker_request_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "request_type".to_string(),
            JsonSchema::String {
                description: Some(
                    "Structured request type such as NeedCompile, NeedReview, NeedDecision, NeedGPU, NeedPort, NeedMoreBudget, BlockedByLease, or BlockedByFileConflict."
                        .to_string(),
                ),
            },
        ),
        (
            "reason".to_string(),
            JsonSchema::String {
                description: Some("Short reason for the coordinator queue.".to_string()),
            },
        ),
        (
            "blocking".to_string(),
            JsonSchema::Boolean {
                description: Some(
                    "Whether the worker is blocked until the coordinator resolves this request."
                        .to_string(),
                ),
            },
        ),
        (
            "requested_resource".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional lease/resource key, port, GPU id, command, or decision target."
                        .to_string(),
                ),
            },
        ),
        (
            "artifact_refs".to_string(),
            JsonSchema::Array {
                items: Box::new(JsonSchema::String { description: None }),
                description: Some("Optional artifact ids related to the request.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "submit_worker_request".to_string(),
        description: "Submit a structured AgentOS worker request to the active coordinator queue. Use this instead of asking another worker directly.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["request_type".to_string(), "reason".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(submit_worker_request_output_schema()),
    })
}

pub fn create_update_worker_request_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "request_id".to_string(),
            JsonSchema::String {
                description: Some("AgentOS worker request id.".to_string()),
            },
        ),
        (
            "status".to_string(),
            JsonSchema::String {
                description: Some(
                    "New request status: Pending, Approved, Rejected, Resolved, or Cancelled."
                        .to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "update_worker_request".to_string(),
        description: "Update a structured AgentOS worker request. The owner or active rank-0 coordinator may resolve it.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["request_id".to_string(), "status".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(update_worker_request_output_schema()),
    })
}

pub fn create_update_runtime_command_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "command_id".to_string(),
            JsonSchema::String {
                description: Some("AgentOS runtime command id.".to_string()),
            },
        ),
        (
            "status".to_string(),
            JsonSchema::String {
                description: Some(
                    "New command status: Acked, Executing, Completed, Failed, or Rejected."
                        .to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "update_runtime_command".to_string(),
        description: "Update an AgentOS RuntimeCommand status from the sender or receiver thread."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["command_id".to_string(), "status".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(update_runtime_command_output_schema()),
    })
}

pub fn create_poll_runtime_commands_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "auto_ack".to_string(),
        JsonSchema::Boolean {
            description: Some(
                "When true or omitted, pending commands for this thread are marked Acked as they are returned."
                    .to_string(),
            ),
        },
    )]);

    ToolSpec::Function(ResponsesApiTool {
        name: "poll_runtime_commands".to_string(),
        description: "Poll this thread's AgentOS RuntimeCommand inbox. Stale or expired commands are rejected by the runtime before results are returned.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: None,
            additional_properties: Some(false.into()),
        },
        output_schema: Some(poll_runtime_commands_output_schema()),
    })
}

pub fn create_close_agent_tool() -> ToolSpec {
    let properties = BTreeMap::from([("target".to_string(), agent_target_schema("close"))]);

    ToolSpec::Function(ResponsesApiTool {
        name: "close_agent".to_string(),
        description: "Close an agent and any open descendants when they are no longer needed, and return the target agent's previous status before shutdown was requested. Don't keep agents open for too long if they are not needed anymore.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["target".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(close_agent_output_schema()),
    })
}

fn agent_status_output_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "string",
                "enum": ["pending_init", "running", "interrupted", "shutdown", "not_found"]
            },
            {
                "type": "object",
                "properties": {
                    "completed": {
                        "type": ["string", "null"]
                    }
                },
                "required": ["completed"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "errored": {
                        "type": "string"
                    }
                },
                "required": ["errored"],
                "additionalProperties": false
            }
        ]
    })
}

fn spawn_agent_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "agent_id": {
                "type": ["string", "null"],
                "description": "Stable thread identifier for the spawned agent. Prefer this as the target for wait_agent, assign_task, send_message, and close_agent."
            },
            "task_name": {
                "type": "string",
                "description": "Canonical task name for the spawned agent."
            },
            "agent_base_name": {
                "type": ["string", "null"],
                "description": "Chinese base name assigned by Praxis, for example `墨子`."
            },
            "agent_title": {
                "type": ["string", "null"],
                "description": "Short responsibility title supplied at spawn time, for example `负责GUI`."
            },
            "agent_display_name": {
                "type": ["string", "null"],
                "description": "User-facing display name combining base name and title, for example `墨子-负责GUI`."
            },
            "recommended_target": {
                "type": "string",
                "description": "Best target string to reuse for follow-up tools. Usually this is the stable thread id."
            },
            "next_action": {
                "type": "string",
                "description": "Plain-language next step for coordinating this spawned worker."
            }
        },
        "required": ["agent_id", "task_name", "agent_base_name", "agent_title", "agent_display_name", "recommended_target", "next_action"],
        "additionalProperties": false
    })
}

fn message_submission_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "submission_id": {
                "type": "string",
                "description": "Identifier for the queued input submission."
            },
            "runtime_command_id": {
                "type": ["string", "null"],
                "description": "AgentOS RuntimeCommand id for structured assign_task dispatches."
            },
            "target": {
                "type": "string",
                "description": "Original target string requested by the caller."
            },
            "target_thread_id": {
                "type": "string",
                "description": "Resolved stable target thread id. Prefer this for the next wait_agent or assign_task call."
            },
            "target_agent_base_name": {
                "type": ["string", "null"],
                "description": "Resolved Chinese base name for the target, when available."
            },
            "target_agent_title": {
                "type": ["string", "null"],
                "description": "Resolved short responsibility title for the target, when available."
            },
            "target_agent_display_name": {
                "type": ["string", "null"],
                "description": "Resolved user-facing display name for the target, when available."
            },
            "delivery_mode": {
                "type": "string",
                "enum": ["send_message", "assign_task"],
                "description": "Whether this call only queued a message or triggered an assigned task turn."
            },
            "next_action": {
                "type": "string",
                "description": "Plain-language next step; assign_task results normally tell the caller to wait_agent on target_thread_id."
            }
        },
        "required": [
            "submission_id",
            "runtime_command_id",
            "target",
            "target_thread_id",
            "target_agent_base_name",
            "target_agent_title",
            "target_agent_display_name",
            "delivery_mode",
            "next_action"
        ],
        "additionalProperties": false
    })
}

fn list_agents_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "agents": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "thread_id": {
                            "type": "string",
                            "description": "Stable thread id for this agent. Prefer this as the target when names are ambiguous."
                        },
                        "recommended_target": {
                            "type": "string",
                            "description": "Stable target string to use for wait_agent, assign_task, send_message, or close_agent. Prefer this over agent_name or display names."
                        },
                        "next_action": {
                            "type": "string",
                            "description": "Plain-language next coordination step for this agent."
                        },
                        "agent_name": {
                            "type": "string",
                            "description": "Canonical task name for the agent when available, otherwise the agent id."
                        },
                        "agent_base_name": {
                            "type": ["string", "null"],
                            "description": "Chinese base name assigned by Praxis, for example `墨子`."
                        },
                        "agent_title": {
                            "type": ["string", "null"],
                            "description": "Short responsibility title assigned to the agent, for example `负责GUI`."
                        },
                        "agent_display_name": {
                            "type": ["string", "null"],
                            "description": "User-facing display name, for example `墨子-负责GUI`, when available."
                        },
                        "agent_role": {
                            "type": ["string", "null"],
                            "description": "Configured agent role when available."
                        },
                        "agent_status": {
                            "description": "Last known status of the agent.",
                            "allOf": [agent_status_output_schema()]
                        },
                        "last_task_message": {
                            "type": ["string", "null"],
                            "description": "Most recent user or inter-agent instruction received by the agent, when available."
                        }
                    },
                    "required": ["thread_id", "recommended_target", "next_action", "agent_name", "agent_base_name", "agent_title", "agent_display_name", "agent_role", "agent_status", "last_task_message"],
                    "additionalProperties": false
                },
                "description": "Live sub-agents visible in the current root thread tree. The current `/root` main agent is omitted."
            },
            "terminal_state": {
                "type": "object",
                "properties": {
                    "only_root": {
                        "type": "boolean",
                        "description": "True when the unfiltered registry only contained `/root`, the current main agent."
                    },
                    "no_live_subagents": {
                        "type": "boolean",
                        "description": "True when no returned row represents a live sub-agent."
                    },
                    "no_pending_agent_os_work": {
                        "type": "boolean",
                        "description": "True when AgentOS has no leases, pending worker requests, or pending runtime commands."
                    },
                    "should_stop_listing": {
                        "type": "boolean",
                        "description": "True when repeated list_agents calls are useless; summarize instead."
                    },
                    "message": {
                        "type": "string",
                        "description": "Plain-language instruction for what the agent should do next."
                    }
                },
                "required": [
                    "only_root",
                    "no_live_subagents",
                    "no_pending_agent_os_work",
                    "should_stop_listing",
                    "message"
                ],
                "additionalProperties": false
            },
            "agent_os": {
                "type": "object",
                "properties": {
                    "leases": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "lease_id": { "type": "string" },
                                "resource_type": { "type": "string" },
                                "scope": { "type": "string" },
                                "mode": { "type": "string" },
                                "owner_thread_id": { "type": "string" },
                                "task_id": { "type": "string" },
                                "priority": { "type": "integer" },
                                "expires_at": { "type": ["string", "null"] }
                            },
                            "required": [
                                "lease_id",
                                "resource_type",
                                "scope",
                                "mode",
                                "owner_thread_id",
                                "task_id",
                                "priority",
                                "expires_at"
                            ],
                            "additionalProperties": false
                        }
                    },
                    "recent_artifacts": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "artifact_id": { "type": "string" },
                                "task_id": { "type": "string" },
                                "owner_thread_id": { "type": "string" },
                                "artifact_type": { "type": "string" },
                                "uri": { "type": "string" },
                                "summary": { "type": "string" },
                                "blob_persisted": { "type": "boolean" },
                                "blob_bytes": { "type": ["integer", "null"] },
                                "blob_path": { "type": ["string", "null"] },
                                "created_at": { "type": "string" }
                            },
                            "required": [
                                "artifact_id",
                                "task_id",
                                "owner_thread_id",
                                "artifact_type",
                                "uri",
                                "summary",
                                "blob_persisted",
                                "blob_bytes",
                                "blob_path",
                                "created_at"
                            ],
                            "additionalProperties": false
                        }
                    },
                    "pending_worker_requests": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "request_id": { "type": "string" },
                                "request_type": { "type": "string" },
                                "thread_id": { "type": "string" },
                                "task_id": { "type": ["string", "null"] },
                                "blocking": { "type": "boolean" },
                                "status": { "type": "string" },
                                "reason": { "type": "string" },
                                "requested_resource": { "type": ["string", "null"] },
                                "artifact_refs": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "created_at": { "type": "string" }
                            },
                            "required": [
                                "request_id",
                                "request_type",
                                "thread_id",
                                "task_id",
                                "blocking",
                                "status",
                                "reason",
                                "requested_resource",
                                "artifact_refs",
                                "created_at"
                            ],
                            "additionalProperties": false
                        }
                    },
                    "pending_runtime_commands": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "command_id": { "type": "string" },
                                "from_thread_id": { "type": "string" },
                                "to_thread_id": { "type": "string" },
                                "task_id": { "type": ["string", "null"] },
                                "command_type": { "type": "string" },
                                "status": { "type": "string" },
                                "coordinator_epoch": { "type": "integer" },
                                "fencing_token": { "type": "integer" },
                                "created_at": { "type": "string" },
                                "expires_at": { "type": "string" }
                            },
                            "required": [
                                "command_id",
                                "from_thread_id",
                                "to_thread_id",
                                "task_id",
                                "command_type",
                                "status",
                                "coordinator_epoch",
                                "fencing_token",
                                "created_at",
                                "expires_at"
                            ],
                            "additionalProperties": false
                        }
                    },
                    "recent_intent_plans": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "plan_id": { "type": "string" },
                                "task_id": { "type": "string" },
                                "thread_id": { "type": "string" },
                                "intent": { "type": "string" },
                                "confidence": { "type": "number" },
                                "command_fingerprint": { "type": "string" },
                                "cwd": { "type": "string" },
                                "required_capabilities": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "required_resources": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                },
                                "risk_level": { "type": "string" },
                                "status": { "type": "string" },
                                "consumed_by_ticket_id": { "type": ["string", "null"] },
                                "created_at": { "type": "string" },
                                "expires_at": { "type": "string" }
                            },
                            "required": [
                                "plan_id",
                                "task_id",
                                "thread_id",
                                "intent",
                                "confidence",
                                "command_fingerprint",
                                "cwd",
                                "required_capabilities",
                                "required_resources",
                                "risk_level",
                                "status",
                                "consumed_by_ticket_id",
                                "created_at",
                                "expires_at"
                            ],
                            "additionalProperties": false
                        }
                    }
                },
                "required": [
                    "leases",
                    "recent_artifacts",
                    "pending_worker_requests",
                    "pending_runtime_commands",
                    "recent_intent_plans"
                ],
                "additionalProperties": false
            }
        },
        "required": ["agents", "agent_os", "terminal_state"],
        "additionalProperties": false
    })
}

fn read_agent_artifact_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "artifact_id": {
                "type": "string"
            },
            "task_id": {
                "type": "string"
            },
            "owner_thread_id": {
                "type": "string"
            },
            "artifact_type": {
                "type": "string"
            },
            "uri": {
                "type": "string"
            },
            "content": {
                "type": "string"
            },
            "bytes_read": {
                "type": "integer"
            },
            "blob_bytes": {
                "type": ["integer", "null"]
            },
            "truncated": {
                "type": "boolean"
            }
        },
        "required": [
            "artifact_id",
            "task_id",
            "owner_thread_id",
            "artifact_type",
            "uri",
            "content",
            "bytes_read",
            "blob_bytes",
            "truncated"
        ],
        "additionalProperties": false
    })
}

fn submit_worker_request_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "request_id": { "type": "string" },
            "request_type": { "type": "string" },
            "thread_id": { "type": "string" },
            "task_id": { "type": ["string", "null"] },
            "blocking": { "type": "boolean" },
            "status": { "type": "string" },
            "reason": { "type": "string" },
            "requested_resource": { "type": ["string", "null"] },
            "artifact_refs": {
                "type": "array",
                "items": { "type": "string" }
            },
            "created_at": { "type": "string" }
        },
        "required": [
            "request_id",
            "request_type",
            "thread_id",
            "task_id",
            "blocking",
            "status",
            "reason",
            "requested_resource",
            "artifact_refs",
            "created_at"
        ],
        "additionalProperties": false
    })
}

fn update_worker_request_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "request_id": { "type": "string" },
            "request_type": { "type": "string" },
            "thread_id": { "type": "string" },
            "task_id": { "type": ["string", "null"] },
            "blocking": { "type": "boolean" },
            "status": { "type": "string" },
            "updated_at": { "type": "string" }
        },
        "required": [
            "request_id",
            "request_type",
            "thread_id",
            "task_id",
            "blocking",
            "status",
            "updated_at"
        ],
        "additionalProperties": false
    })
}

fn update_runtime_command_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command_id": { "type": "string" },
            "from_thread_id": { "type": "string" },
            "to_thread_id": { "type": "string" },
            "task_id": { "type": ["string", "null"] },
            "command_type": { "type": "string" },
            "status": { "type": "string" },
            "updated_at": { "type": "string" }
        },
        "required": [
            "command_id",
            "from_thread_id",
            "to_thread_id",
            "task_id",
            "command_type",
            "status",
            "updated_at"
        ],
        "additionalProperties": false
    })
}

fn poll_runtime_commands_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "commands": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "command_id": { "type": "string" },
                        "from_thread_id": { "type": "string" },
                        "to_thread_id": { "type": "string" },
                        "task_id": { "type": ["string", "null"] },
                        "command_type": { "type": "string" },
                        "payload": { "type": "object" },
                        "status": { "type": "string" },
                        "coordinator_epoch": { "type": "integer" },
                        "fencing_token": { "type": "integer" },
                        "created_at": { "type": "string" },
                        "updated_at": { "type": "string" },
                        "expires_at": { "type": "string" }
                    },
                    "required": [
                        "command_id",
                        "from_thread_id",
                        "to_thread_id",
                        "task_id",
                        "command_type",
                        "payload",
                        "status",
                        "coordinator_epoch",
                        "fencing_token",
                        "created_at",
                        "updated_at",
                        "expires_at"
                    ],
                    "additionalProperties": false
                }
            }
        },
        "required": ["commands"],
        "additionalProperties": false
    })
}

fn wait_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {
                "type": "string",
                "description": "Brief wait summary."
            },
            "timed_out": {
                "type": "boolean",
                "description": "Whether the wait call returned due to timeout."
            },
            "source": {
                "type": "string",
                "enum": ["mailbox", "agent_os", "target_status", "timeout"],
                "description": "Why wait_agent returned."
            },
            "agent_os_sequence": {
                "type": ["integer", "null"],
                "description": "AgentOS change sequence observed when wait_agent returned."
            },
            "target": {
                "type": "string",
                "description": "Requested target when target mode was used."
            },
            "target_thread_id": {
                "type": "string",
                "description": "Resolved target thread id when target mode was used."
            },
            "target_agent_base_name": {
                "type": "string",
                "description": "Resolved target Chinese base name when target mode was used and available."
            },
            "target_agent_title": {
                "type": "string",
                "description": "Resolved target responsibility title when target mode was used and available."
            },
            "target_agent_display_name": {
                "type": "string",
                "description": "Resolved target display name when target mode was used and available."
            },
            "target_status": {
                "description": "Resolved target's status when target mode was used.",
                "allOf": [agent_status_output_schema()]
            },
            "next_action": {
                "type": "string",
                "description": "Plain-language next step for worker coordination."
            }
        },
        "required": ["message", "timed_out", "source", "agent_os_sequence", "next_action"],
        "additionalProperties": false
    })
}

fn close_agent_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "previous_status": {
                "description": "The agent status observed before shutdown was requested.",
                "allOf": [agent_status_output_schema()]
            }
        },
        "required": ["previous_status"],
        "additionalProperties": false
    })
}

fn spawn_agent_common_properties(agent_type_description: &str) -> BTreeMap<String, JsonSchema> {
    BTreeMap::from([
        (
            "message".to_string(),
            JsonSchema::String {
                description: Some("Initial plain-text task for the new agent.".to_string()),
            },
        ),
        (
            "agent_type".to_string(),
            JsonSchema::String {
                description: Some(agent_type_description.to_string()),
            },
        ),
        (
            "fork_turns".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional fork mode. Use `none`, `all`, or a positive integer string such as `3` to fork only the most recent turns."
                        .to_string(),
                ),
            },
        ),
        (
            "model_provider".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional model provider override for the new agent. Use provider ids such as `openai`, `deepseek`, `qwen`, `glm`, or `common`. `codex` and `responses` are accepted aliases for OpenAI/Codex workers. When omitted, the provider is inferred from a known first-party model when possible, otherwise inherited."
                        .to_string(),
                ),
            },
        ),
        (
            "model".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional model override for the new agent. Replaces the inherited model; known first-party model ids can also switch the provider automatically. For the strongest Codex coding worker, use `gpt-5.5`; natural aliases like `5.5`, `gpt5.5`, and `codex5.5` are accepted."
                        .to_string(),
                ),
            },
        ),
        (
            "reasoning_effort".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional reasoning effort override for the new agent. Replaces the inherited reasoning effort. Use `xhigh` for maximum reasoning; aliases like `x-high`, `extra high`, and `maximum` are accepted."
                        .to_string(),
                ),
            },
        ),
    ])
}

fn spawn_agent_tool_description(
    available_models_description: &str,
    return_value_description: &str,
) -> String {
    format!(
        r#"
        Only use `spawn_agent` if and only if the user explicitly asks for sub-agents, delegation, or parallel agent work.
        Requests for depth, thoroughness, research, investigation, or detailed codebase analysis do not count as permission to spawn.
        Agent-role guidance below only helps choose which agent to use after spawning is already authorized; it never authorizes spawning by itself.
        Spawn a sub-agent for a well-scoped task. {return_value_description} This spawn_agent tool provides you access to smaller but more efficient sub-agents. A mini model can solve many tasks faster than the main model. You should follow the rules and guidelines below to use this tool.

{available_models_description}
### Cross-provider coding workers
- Picker-visible models above may only reflect the current provider. When a Codex/GPT worker is needed and the OpenAI provider is configured, you may still explicitly set `model_provider` to `openai`, `model` to `gpt-5.5`, and `reasoning_effort` to `xhigh`. Natural model aliases such as `5.5`, `gpt5.5`, `codex5.5`, and `gpt 5.5 xhigh` are accepted, but explicit fields are more reliable.

### When to delegate vs. do the subtask yourself
- First, quickly analyze the overall user task and form a succinct high-level plan. Identify which tasks are immediate blockers on the critical path, and which tasks are sidecar tasks that are needed but can run in parallel without blocking the next local step. As part of that plan, explicitly decide what immediate task you should do locally right now. Do this planning step before delegating to agents so you do not hand off the immediate blocking task to a submodel and then waste time waiting on it.
- Use the smaller subagent when a subtask is easy enough for it to handle and can run in parallel with your local work. Prefer delegating concrete, bounded sidecar tasks that materially advance the main task without blocking your immediate next local step.
- Do not delegate urgent blocking work when your immediate next step depends on that result. If the very next action is blocked on that task, the main rollout should usually do it locally to keep the critical path moving.
- Keep work local when the subtask is too difficult to delegate well and when it is tightly coupled, urgent, or likely to block your immediate next step.

### Designing delegated subtasks
- Subtasks must be concrete, well-defined, and self-contained.
- Delegated subtasks must materially advance the main task.
- Provide `task_name` as the lowercase ASCII canonical tool reference. Also provide `title` when you know a concise human-facing responsibility label; Praxis can derive it from `task_name` if omitted, but an explicit title renders better as a Chinese display name such as `墨子-负责GUI`. `title` is the responsibility, not the agent name; never set it to `墨子`, `荀子`, or another base name by itself.
- Do not duplicate work between the main rollout and delegated subtasks.
- Avoid issuing multiple delegate calls on the same unresolved thread unless the new delegated task is genuinely different and necessary.
- Narrow the delegated ask to the concrete output you need next.
- For coding tasks, prefer delegating concrete code-change worker subtasks over read-only explorer analysis when the subagent can make a bounded patch in a clear write scope.
- When delegating coding work, instruct the submodel to edit files directly in its forked workspace and list the file paths it changed in the final answer.
- For code-edit subtasks, decompose work so each delegated task has a disjoint write set.

### After you delegate
- Call wait_agent very sparingly. Only call wait_agent when you need the result immediately for the next critical-path step and you are blocked until it returns.
- Do not redo delegated subagent tasks yourself; focus on integrating results or tackling non-overlapping work.
- While the subagent is running in the background, do meaningful non-overlapping work immediately.
- Do not repeatedly wait by reflex.
- When a delegated coding task returns, quickly review the uploaded changes, then integrate or refine them.

### Parallel delegation patterns
- Run multiple independent information-seeking subtasks in parallel when you have distinct questions that can be answered independently.
- Split implementation into disjoint codebase slices and spawn multiple agents for them in parallel when the write scopes do not overlap.
- Delegate verification only when it can run in parallel with ongoing implementation and is likely to catch a concrete risk before final integration.
- The key is to find opportunities to spawn multiple independent subtasks in parallel within the same round, while ensuring each subtask is well-defined, self-contained, and materially advances the main task."#
    )
}

fn spawn_agent_models_description(models: &[ModelPreset]) -> String {
    let visible_models: Vec<&ModelPreset> =
        models.iter().filter(|model| model.show_in_picker).collect();
    if visible_models.is_empty() {
        return "No picker-visible models are currently loaded.".to_string();
    }

    visible_models
        .into_iter()
        .map(|model| {
            let efforts = model
                .supported_reasoning_efforts
                .iter()
                .map(|preset| format!("{} ({})", preset.effort, preset.description))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "- {} (`{}`): {} Default reasoning effort: {}. Supported reasoning efforts: {}.",
                model.display_name,
                model.model,
                model.description,
                model.default_reasoning_effort,
                efforts
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn wait_agent_tool_parameters(options: WaitAgentTimeoutOptions) -> JsonSchema {
    let properties = BTreeMap::from([
        ("target".to_string(), agent_target_schema("wait on")),
        (
            "timeout_ms".to_string(),
            JsonSchema::Number {
                description: Some(format!(
                    "Optional timeout in milliseconds. Defaults to {}, min {}, max {}. Prefer longer waits (minutes) to avoid busy polling.",
                    options.default_timeout_ms, options.min_timeout_ms, options.max_timeout_ms,
                )),
            },
        ),
    ]);

    JsonSchema::Object {
        properties,
        required: None,
        additional_properties: Some(false.into()),
    }
}

#[cfg(test)]
#[path = "agent_tool_tests.rs"]
mod tests;
