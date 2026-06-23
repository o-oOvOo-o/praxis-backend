use serde_json::Value;
use serde_json::json;

pub(super) fn agent_status_output_schema() -> Value {
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

pub(super) fn spawn_agent_output_schema() -> Value {
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

pub(super) fn message_submission_output_schema() -> Value {
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

pub(super) fn list_agents_output_schema() -> Value {
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

pub(super) fn read_agent_artifact_output_schema() -> Value {
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

pub(super) fn submit_worker_request_output_schema() -> Value {
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

pub(super) fn update_worker_request_output_schema() -> Value {
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

pub(super) fn update_runtime_command_output_schema() -> Value {
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

pub(super) fn poll_runtime_commands_output_schema() -> Value {
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

pub(super) fn wait_output_schema() -> Value {
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

pub(super) fn close_agent_output_schema() -> Value {
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
