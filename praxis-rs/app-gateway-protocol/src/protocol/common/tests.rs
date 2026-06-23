use super::*;
use anyhow::Result;
use praxis_protocol::ThreadId;
use praxis_protocol::account::PlanType;
use praxis_protocol::protocol::RealtimeConversationVersion;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::PathBuf;

fn absolute_path_string(path: &str) -> String {
    let trimmed = path.trim_start_matches('/');
    if cfg!(windows) {
        format!(r"C:\{}", trimmed.replace('/', "\\"))
    } else {
        format!("/{trimmed}")
    }
}

fn absolute_path(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(absolute_path_string(path)).expect("absolute path")
}

#[test]
fn serialize_initialize_with_opt_out_notification_methods() -> Result<()> {
    let request = ClientRequest::Initialize {
        request_id: RequestId::Integer(42),
        params: api::InitializeParams {
            client_info: api::ClientInfo {
                name: "praxis_vscode".to_string(),
                title: Some("Praxis VS Code Extension".to_string()),
                version: "0.1.0".to_string(),
            },
            capabilities: Some(api::InitializeCapabilities {
                experimental_api: true,
                opt_out_notification_methods: Some(vec![
                    "thread/started".to_string(),
                    "item/agentMessage/delta".to_string(),
                ]),
            }),
            host_extensions: Vec::new(),
        },
    };

    assert_eq!(
        json!({
            "method": "initialize",
            "id": 42,
            "params": {
                "clientInfo": {
                    "name": "praxis_vscode",
                    "title": "Praxis VS Code Extension",
                    "version": "0.1.0"
                },
                "capabilities": {
                    "experimentalApi": true,
                    "optOutNotificationMethods": [
                        "thread/started",
                        "item/agentMessage/delta"
                    ]
                }
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn deserialize_initialize_with_opt_out_notification_methods() -> Result<()> {
    let request: ClientRequest = serde_json::from_value(json!({
        "method": "initialize",
        "id": 42,
        "params": {
            "clientInfo": {
                "name": "praxis_vscode",
                "title": "Praxis VS Code Extension",
                "version": "0.1.0"
            },
            "capabilities": {
                "experimentalApi": true,
                "optOutNotificationMethods": [
                    "thread/started",
                    "item/agentMessage/delta"
                ]
            }
        }
    }))?;

    assert_eq!(
        request,
        ClientRequest::Initialize {
            request_id: RequestId::Integer(42),
            params: api::InitializeParams {
                client_info: api::ClientInfo {
                    name: "praxis_vscode".to_string(),
                    title: Some("Praxis VS Code Extension".to_string()),
                    version: "0.1.0".to_string(),
                },
                capabilities: Some(api::InitializeCapabilities {
                    experimental_api: true,
                    opt_out_notification_methods: Some(vec![
                        "thread/started".to_string(),
                        "item/agentMessage/delta".to_string(),
                    ]),
                }),
                host_extensions: Vec::new(),
            },
        }
    );
    Ok(())
}

#[test]
fn conversation_id_serializes_as_plain_string() -> Result<()> {
    let id = ThreadId::from_string("67e55044-10b1-426f-9247-bb680e5fe0c8")?;

    assert_eq!(
        json!("67e55044-10b1-426f-9247-bb680e5fe0c8"),
        serde_json::to_value(id)?
    );
    Ok(())
}

#[test]
fn conversation_id_deserializes_from_plain_string() -> Result<()> {
    let id: ThreadId = serde_json::from_value(json!("67e55044-10b1-426f-9247-bb680e5fe0c8"))?;

    assert_eq!(
        ThreadId::from_string("67e55044-10b1-426f-9247-bb680e5fe0c8")?,
        id,
    );
    Ok(())
}

#[test]
fn serialize_client_notification() -> Result<()> {
    let notification = ClientNotification::Initialized;
    // Note there is no "params" field for this notification.
    assert_eq!(
        json!({
            "method": "initialized",
        }),
        serde_json::to_value(&notification)?,
    );
    Ok(())
}

#[test]
fn serialize_chatgpt_auth_tokens_refresh_request() -> Result<()> {
    let request = ServerRequest::ChatgptAuthTokensRefresh {
        request_id: RequestId::Integer(8),
        params: api::ChatgptAuthTokensRefreshParams {
            reason: api::ChatgptAuthTokensRefreshReason::Unauthorized,
            previous_account_id: Some("org-123".to_string()),
        },
    };
    assert_eq!(
        json!({
            "method": "account/chatgptAuthTokens/refresh",
            "id": 8,
            "params": {
                "reason": "unauthorized",
                "previousAccountId": "org-123"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_mcp_server_elicitation_request() -> Result<()> {
    let requested_schema: api::McpElicitationSchema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            "confirmed": {
                "type": "boolean"
            }
        },
        "required": ["confirmed"]
    }))?;
    let params = api::McpServerElicitationRequestParams {
        thread_id: "thr_123".to_string(),
        turn_id: Some("turn_123".to_string()),
        server_name: "praxis_apps".to_string(),
        request: api::McpServerElicitationRequest::Form {
            meta: None,
            message: "Allow this request?".to_string(),
            requested_schema,
        },
    };
    let request = ServerRequest::McpServerElicitationRequest {
        request_id: RequestId::Integer(9),
        params: params.clone(),
    };

    assert_eq!(
        json!({
            "method": "mcpServer/elicitation/request",
            "id": 9,
            "params": {
                "threadId": "thr_123",
                "turnId": "turn_123",
                "serverName": "praxis_apps",
                "mode": "form",
                "_meta": null,
                "message": "Allow this request?",
                "requestedSchema": {
                    "type": "object",
                    "properties": {
                        "confirmed": {
                            "type": "boolean"
                        }
                    },
                    "required": ["confirmed"]
                }
            }
        }),
        serde_json::to_value(&request)?,
    );

    let payload = ServerRequestPayload::McpServerElicitationRequest(params);
    assert_eq!(request.id(), &RequestId::Integer(9));
    assert_eq!(payload.request_with_id(RequestId::Integer(9)), request);
    Ok(())
}

#[test]
fn serialize_get_account_rate_limits() -> Result<()> {
    let request = ClientRequest::GetAccountRateLimits {
        request_id: RequestId::Integer(1),
        params: None,
    };
    assert_eq!(request.id(), &RequestId::Integer(1));
    assert_eq!(request.method(), "account/rateLimits/read");
    assert_eq!(
        json!({
            "method": "account/rateLimits/read",
            "id": 1,
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_client_response() -> Result<()> {
    let response = ClientResponse::ThreadStart {
        request_id: RequestId::Integer(7),
        response: api::ThreadStartResponse {
            thread: api::Thread {
                id: "67e55044-10b1-426f-9247-bb680e5fe0c8".to_string(),
                preview: "first prompt".to_string(),
                summary: None,
                ephemeral: true,
                model_provider: "openai".to_string(),
                model: Some("gpt-5".to_string()),
                created_at: 1,
                updated_at: 2,
                status: api::ThreadStatus::Idle,
                path: None,
                cwd: PathBuf::from("/tmp"),
                cli_version: "0.0.0".to_string(),
                source: api::SessionSource::Exec,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                git_info: None,
                name: None,
                total_cost_usd: None,
                last_cost_usd: None,
                token_usage: None,
                control_state: None,
                selfwork_plan_path: None,
                turns: Vec::new(),
            },
            model: "gpt-5".to_string(),
            model_provider: "openai".to_string(),
            service_tier: None,
            cwd: PathBuf::from("/tmp"),
            approval_policy: api::AskForApproval::OnFailure,
            approvals_reviewer: api::ApprovalsReviewer::User,
            sandbox: api::SandboxPolicy::DangerFullAccess,
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
        },
    };

    assert_eq!(response.id(), &RequestId::Integer(7));
    assert_eq!(response.method(), "thread/start");
    assert_eq!(
        json!({
            "method": "thread/start",
            "id": 7,
            "response": {
                "thread": {
                    "id": "67e55044-10b1-426f-9247-bb680e5fe0c8",
                    "preview": "first prompt",
                    "ephemeral": true,
                    "modelProvider": "openai",
                    "createdAt": 1,
                    "updatedAt": 2,
                    "status": {
                        "type": "idle"
                    },
                    "path": null,
                    "cwd": "/tmp",
                    "cliVersion": "0.0.0",
                    "source": "exec",
                    "agentNickname": null,
                    "agentRole": null,
                    "gitInfo": null,
                    "name": null,
                    "turns": []
                },
                "model": "gpt-5",
                "modelProvider": "openai",
                "serviceTier": null,
                "cwd": "/tmp",
                "approvalPolicy": "on-failure",
                "approvalsReviewer": "user",
                "sandbox": {
                    "type": "dangerFullAccess"
                },
                "reasoningEffort": null,
                "historyLogId": 0,
                "historyEntryCount": 0
            }
        }),
        serde_json::to_value(&response)?,
    );
    Ok(())
}

#[test]
fn serialize_config_requirements_read() -> Result<()> {
    let request = ClientRequest::ConfigRequirementsRead {
        request_id: RequestId::Integer(1),
        params: None,
    };
    assert_eq!(
        json!({
            "method": "configRequirements/read",
            "id": 1,
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_account_login_api_key() -> Result<()> {
    let request = ClientRequest::LoginAccount {
        request_id: RequestId::Integer(2),
        params: api::LoginAccountParams::ApiKey {
            api_key: "secret".to_string(),
        },
    };
    assert_eq!(
        json!({
            "method": "account/login/start",
            "id": 2,
            "params": {
                "type": "apiKey",
                "apiKey": "secret"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_account_login_chatgpt() -> Result<()> {
    let request = ClientRequest::LoginAccount {
        request_id: RequestId::Integer(3),
        params: api::LoginAccountParams::Chatgpt,
    };
    assert_eq!(
        json!({
            "method": "account/login/start",
            "id": 3,
            "params": {
                "type": "chatgpt"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_account_login_chatgpt_device_code() -> Result<()> {
    let request = ClientRequest::LoginAccount {
        request_id: RequestId::Integer(4),
        params: api::LoginAccountParams::ChatgptDeviceCode,
    };
    assert_eq!(
        json!({
            "method": "account/login/start",
            "id": 4,
            "params": {
                "type": "chatgptDeviceCode"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_account_logout() -> Result<()> {
    let request = ClientRequest::LogoutAccount {
        request_id: RequestId::Integer(5),
        params: None,
    };
    assert_eq!(
        json!({
            "method": "account/logout",
            "id": 5,
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_account_login_chatgpt_auth_tokens() -> Result<()> {
    let request = ClientRequest::LoginAccount {
        request_id: RequestId::Integer(6),
        params: api::LoginAccountParams::ChatgptAuthTokens {
            access_token: "access-token".to_string(),
            chatgpt_account_id: "org-123".to_string(),
            chatgpt_plan_type: Some("business".to_string()),
        },
    };
    assert_eq!(
        json!({
            "method": "account/login/start",
            "id": 6,
            "params": {
                "type": "chatgptAuthTokens",
                "accessToken": "access-token",
                "chatgptAccountId": "org-123",
                "chatgptPlanType": "business"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_get_account() -> Result<()> {
    let request = ClientRequest::GetAccount {
        request_id: RequestId::Integer(6),
        params: api::GetAccountParams {
            refresh_token: false,
        },
    };
    assert_eq!(
        json!({
            "method": "account/read",
            "id": 6,
            "params": {
                "refreshToken": false
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn account_serializes_fields_in_camel_case() -> Result<()> {
    let api_key = api::Account::ApiKey {};
    assert_eq!(
        json!({
            "type": "apiKey",
        }),
        serde_json::to_value(&api_key)?,
    );

    let chatgpt = api::Account::Chatgpt {
        email: "user@example.com".to_string(),
        plan_type: PlanType::Plus,
    };
    assert_eq!(
        json!({
            "type": "chatgpt",
            "email": "user@example.com",
            "planType": "plus",
        }),
        serde_json::to_value(&chatgpt)?,
    );

    Ok(())
}

#[test]
fn serialize_list_models() -> Result<()> {
    let request = ClientRequest::ModelList {
        request_id: RequestId::Integer(6),
        params: api::ModelListParams::default(),
    };
    assert_eq!(
        json!({
            "method": "model/list",
            "id": 6,
            "params": {
                "limit": null,
                "cursor": null,
                "includeHidden": null
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_list_collaboration_modes() -> Result<()> {
    let request = ClientRequest::CollaborationModeList {
        request_id: RequestId::Integer(7),
        params: api::CollaborationModeListParams::default(),
    };
    assert_eq!(
        json!({
            "method": "collaborationMode/list",
            "id": 7,
            "params": {}
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_list_apps() -> Result<()> {
    let request = ClientRequest::AppsList {
        request_id: RequestId::Integer(8),
        params: api::AppsListParams::default(),
    };
    assert_eq!(
        json!({
            "method": "app/list",
            "id": 8,
            "params": {
                "cursor": null,
                "limit": null,
                "threadId": null
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_fs_get_metadata() -> Result<()> {
    let request = ClientRequest::FsGetMetadata {
        request_id: RequestId::Integer(9),
        params: api::FsGetMetadataParams {
            path: absolute_path("tmp/example"),
        },
    };
    assert_eq!(
        json!({
            "method": "fs/getMetadata",
            "id": 9,
            "params": {
                "path": absolute_path_string("tmp/example")
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_fs_watch() -> Result<()> {
    let request = ClientRequest::FsWatch {
        request_id: RequestId::Integer(10),
        params: api::FsWatchParams {
            path: absolute_path("tmp/repo/.git"),
        },
    };
    assert_eq!(
        json!({
            "method": "fs/watch",
            "id": 10,
            "params": {
                "path": absolute_path_string("tmp/repo/.git")
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_list_experimental_features() -> Result<()> {
    let request = ClientRequest::ExperimentalFeatureList {
        request_id: RequestId::Integer(8),
        params: api::ExperimentalFeatureListParams::default(),
    };
    assert_eq!(
        json!({
            "method": "experimentalFeature/list",
            "id": 8,
            "params": {
                "cursor": null,
                "limit": null
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_thread_background_terminals_clean() -> Result<()> {
    let request = ClientRequest::ThreadBackgroundTerminalsClean {
        request_id: RequestId::Integer(8),
        params: api::ThreadBackgroundTerminalsCleanParams {
            thread_id: "thr_123".to_string(),
        },
    };
    assert_eq!(
        json!({
            "method": "thread/backgroundTerminals/clean",
            "id": 8,
            "params": {
                "threadId": "thr_123"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_thread_realtime_start() -> Result<()> {
    let request = ClientRequest::ThreadRealtimeStart {
        request_id: RequestId::Integer(9),
        params: api::ThreadRealtimeStartParams {
            thread_id: "thr_123".to_string(),
            prompt: "You are on a call".to_string(),
            session_id: Some("sess_456".to_string()),
        },
    };
    assert_eq!(
        json!({
            "method": "thread/realtime/start",
            "id": 9,
            "params": {
                "threadId": "thr_123",
                "prompt": "You are on a call",
                "sessionId": "sess_456"
            }
        }),
        serde_json::to_value(&request)?,
    );
    Ok(())
}

#[test]
fn serialize_thread_status_changed_notification() -> Result<()> {
    let notification =
        ServerNotification::ThreadStatusChanged(api::ThreadStatusChangedNotification {
            thread_id: "thr_123".to_string(),
            status: api::ThreadStatus::Idle,
        });
    assert_eq!(
        json!({
            "method": "thread/status/changed",
            "params": {
                "threadId": "thr_123",
                "status": {
                    "type": "idle"
                },
            }
        }),
        serde_json::to_value(&notification)?,
    );
    Ok(())
}

#[test]
fn serialize_thread_realtime_output_audio_delta_notification() -> Result<()> {
    let notification = ServerNotification::ThreadRealtimeOutputAudioDelta(
        api::ThreadRealtimeOutputAudioDeltaNotification {
            thread_id: "thr_123".to_string(),
            audio: api::ThreadRealtimeAudioChunk {
                data: "AQID".to_string(),
                sample_rate: 24_000,
                num_channels: 1,
                samples_per_channel: Some(512),
                item_id: None,
            },
        },
    );
    assert_eq!(
        json!({
            "method": "thread/realtime/outputAudio/delta",
            "params": {
                "threadId": "thr_123",
                "audio": {
                    "data": "AQID",
                    "sampleRate": 24000,
                    "numChannels": 1,
                    "samplesPerChannel": 512,
                    "itemId": null
                }
            }
        }),
        serde_json::to_value(&notification)?,
    );
    Ok(())
}

#[test]
fn mock_experimental_method_is_marked_experimental() {
    let request = ClientRequest::MockExperimentalMethod {
        request_id: RequestId::Integer(1),
        params: api::MockExperimentalMethodParams::default(),
    };
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&request);
    assert_eq!(reason, Some("mock/experimentalMethod"));
}
#[test]
fn thread_realtime_start_is_marked_experimental() {
    let request = ClientRequest::ThreadRealtimeStart {
        request_id: RequestId::Integer(1),
        params: api::ThreadRealtimeStartParams {
            thread_id: "thr_123".to_string(),
            prompt: "You are on a call".to_string(),
            session_id: None,
        },
    };
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&request);
    assert_eq!(reason, Some("thread/realtime/start"));
}
#[test]
fn thread_realtime_started_notification_is_marked_experimental() {
    let notification =
        ServerNotification::ThreadRealtimeStarted(api::ThreadRealtimeStartedNotification {
            thread_id: "thr_123".to_string(),
            session_id: Some("sess_456".to_string()),
            version: RealtimeConversationVersion::default(),
        });
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&notification);
    assert_eq!(reason, Some("thread/realtime/started"));
}

#[test]
fn thread_realtime_output_audio_delta_notification_is_marked_experimental() {
    let notification = ServerNotification::ThreadRealtimeOutputAudioDelta(
        api::ThreadRealtimeOutputAudioDeltaNotification {
            thread_id: "thr_123".to_string(),
            audio: api::ThreadRealtimeAudioChunk {
                data: "AQID".to_string(),
                sample_rate: 24_000,
                num_channels: 1,
                samples_per_channel: Some(512),
                item_id: None,
            },
        },
    );
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&notification);
    assert_eq!(reason, Some("thread/realtime/outputAudio/delta"));
}

#[test]
fn command_execution_request_approval_additional_permissions_is_marked_experimental() {
    let params = api::CommandExecutionRequestApprovalParams {
        thread_id: "thr_123".to_string(),
        turn_id: "turn_123".to_string(),
        item_id: "call_123".to_string(),
        approval_id: None,
        reason: None,
        network_approval_context: None,
        command: Some("cat file".to_string()),
        cwd: None,
        command_actions: None,
        additional_permissions: Some(api::AdditionalPermissionProfile {
            network: None,
            file_system: Some(api::AdditionalFileSystemPermissions {
                read: Some(vec![absolute_path("/tmp/allowed")]),
                write: None,
            }),
        }),
        proposed_execpolicy_amendment: None,
        proposed_network_policy_amendments: None,
        available_decisions: None,
    };
    let reason = crate::experimental_api::ExperimentalApi::experimental_reason(&params);
    assert_eq!(
        reason,
        Some("item/commandExecution/requestApproval.additionalPermissions")
    );
}
