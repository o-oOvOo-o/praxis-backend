use super::*;

#[test]
fn collab_agent_state_maps_interrupted_status() {
    assert_eq!(
        CollabAgentState::from(CoreAgentStatus::Interrupted),
        CollabAgentState {
            status: CollabAgentStatus::Interrupted,
            message: None,
        }
    );
}

#[test]
fn command_execution_request_approval_rejects_relative_additional_permission_paths() {
    let err = serde_json::from_value::<CommandExecutionRequestApprovalParams>(json!({
        "threadId": "thr_123",
        "turnId": "turn_123",
        "itemId": "call_123",
        "command": "cat file",
        "cwd": "/tmp",
        "commandActions": null,
        "reason": null,
        "networkApprovalContext": null,
        "additionalPermissions": {
            "network": null,
            "fileSystem": {
                "read": ["relative/path"],
                "write": null
            }
        },
        "proposedExecpolicyAmendment": null,
        "proposedNetworkPolicyAmendments": null,
        "availableDecisions": null
    }))
    .expect_err("relative additional permission paths should fail");
    assert!(
        err.to_string()
            .contains("AbsolutePathBuf deserialized without a base path"),
        "unexpected error: {err}"
    );
}

#[test]
fn permissions_request_approval_uses_request_permission_profile() {
    let read_only_path = if cfg!(windows) {
        r"C:\tmp\read-only"
    } else {
        "/tmp/read-only"
    };
    let read_write_path = if cfg!(windows) {
        r"C:\tmp\read-write"
    } else {
        "/tmp/read-write"
    };
    let params = serde_json::from_value::<PermissionsRequestApprovalParams>(json!({
        "threadId": "thr_123",
        "turnId": "turn_123",
        "itemId": "call_123",
        "reason": "Select a workspace root",
        "permissions": {
            "network": {
                "enabled": true,
            },
            "fileSystem": {
                "read": [read_only_path],
                "write": [read_write_path],
            },
        },
    }))
    .expect("permissions request should deserialize");

    assert_eq!(
        params.permissions,
        RequestPermissionProfile {
            network: Some(AdditionalNetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(AdditionalFileSystemPermissions {
                read: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_only_path))
                        .expect("path must be absolute"),
                ]),
                write: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_write_path))
                        .expect("path must be absolute"),
                ]),
            }),
        }
    );

    assert_eq!(
        CoreRequestPermissionProfile::from(params.permissions),
        CoreRequestPermissionProfile {
            network: Some(CoreNetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(CoreFileSystemPermissions {
                read: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_only_path))
                        .expect("path must be absolute"),
                ]),
                write: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_write_path))
                        .expect("path must be absolute"),
                ]),
            }),
        }
    );
}

#[test]
fn permissions_request_approval_rejects_macos_permissions() {
    let err = serde_json::from_value::<PermissionsRequestApprovalParams>(json!({
        "threadId": "thr_123",
        "turnId": "turn_123",
        "itemId": "call_123",
        "reason": "Select a workspace root",
        "permissions": {
            "network": null,
            "fileSystem": null,
            "macos": {
                "preferences": "read_only",
                "automations": "none",
                "launchServices": false,
                "accessibility": false,
                "calendar": false,
                "reminders": false,
                "contacts": "none",
            },
        },
    }))
    .expect_err("permissions request should reject macos permissions");

    assert!(
        err.to_string().contains("unknown field `macos`"),
        "unexpected error: {err}"
    );
}

#[test]
fn permissions_request_approval_response_uses_granted_permission_profile_without_macos() {
    let read_only_path = if cfg!(windows) {
        r"C:\tmp\read-only"
    } else {
        "/tmp/read-only"
    };
    let read_write_path = if cfg!(windows) {
        r"C:\tmp\read-write"
    } else {
        "/tmp/read-write"
    };
    let response = serde_json::from_value::<PermissionsRequestApprovalResponse>(json!({
        "permissions": {
            "network": {
                "enabled": true,
            },
            "fileSystem": {
                "read": [read_only_path],
                "write": [read_write_path],
            },
        },
    }))
    .expect("permissions response should deserialize");

    assert_eq!(
        response.permissions,
        GrantedPermissionProfile {
            network: Some(AdditionalNetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(AdditionalFileSystemPermissions {
                read: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_only_path))
                        .expect("path must be absolute"),
                ]),
                write: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_write_path))
                        .expect("path must be absolute"),
                ]),
            }),
        }
    );

    assert_eq!(
        CorePermissionProfile::from(response.permissions),
        CorePermissionProfile {
            network: Some(CoreNetworkPermissions {
                enabled: Some(true),
            }),
            file_system: Some(CoreFileSystemPermissions {
                read: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_only_path))
                        .expect("path must be absolute"),
                ]),
                write: Some(vec![
                    AbsolutePathBuf::try_from(PathBuf::from(read_write_path))
                        .expect("path must be absolute"),
                ]),
            }),
        }
    );
}

#[test]
fn permissions_request_approval_response_defaults_scope_to_turn() {
    let response = serde_json::from_value::<PermissionsRequestApprovalResponse>(json!({
        "permissions": {},
    }))
    .expect("response should deserialize");

    assert_eq!(response.scope, PermissionGrantScope::Turn);
}

#[test]
fn fs_get_metadata_response_round_trips_minimal_fields() {
    let response = FsGetMetadataResponse {
        is_directory: false,
        is_file: true,
        created_at_ms: 123,
        modified_at_ms: 456,
    };

    let value = serde_json::to_value(&response).expect("serialize fs/getMetadata response");
    assert_eq!(
        value,
        json!({
            "isDirectory": false,
            "isFile": true,
            "createdAtMs": 123,
            "modifiedAtMs": 456,
        })
    );

    let decoded = serde_json::from_value::<FsGetMetadataResponse>(value)
        .expect("deserialize fs/getMetadata response");
    assert_eq!(decoded, response);
}

#[test]
fn fs_read_file_response_round_trips_base64_data() {
    let response = FsReadFileResponse {
        data_base64: "aGVsbG8=".to_string(),
    };

    let value = serde_json::to_value(&response).expect("serialize fs/readFile response");
    assert_eq!(
        value,
        json!({
            "dataBase64": "aGVsbG8=",
        })
    );

    let decoded = serde_json::from_value::<FsReadFileResponse>(value)
        .expect("deserialize fs/readFile response");
    assert_eq!(decoded, response);
}

#[test]
fn fs_read_file_params_round_trip() {
    let params = FsReadFileParams {
        path: absolute_path("tmp/example.txt"),
    };

    let value = serde_json::to_value(&params).expect("serialize fs/readFile params");
    assert_eq!(
        value,
        json!({
            "path": absolute_path_string("tmp/example.txt"),
        })
    );

    let decoded =
        serde_json::from_value::<FsReadFileParams>(value).expect("deserialize fs/readFile params");
    assert_eq!(decoded, params);
}

#[test]
fn fs_create_directory_params_round_trip_with_default_recursive() {
    let params = FsCreateDirectoryParams {
        path: absolute_path("tmp/example"),
        recursive: None,
    };

    let value = serde_json::to_value(&params).expect("serialize fs/createDirectory params");
    assert_eq!(
        value,
        json!({
            "path": absolute_path_string("tmp/example"),
            "recursive": null,
        })
    );

    let decoded = serde_json::from_value::<FsCreateDirectoryParams>(value)
        .expect("deserialize fs/createDirectory params");
    assert_eq!(decoded, params);
}

#[test]
fn fs_write_file_params_round_trip_with_base64_data() {
    let params = FsWriteFileParams {
        path: absolute_path("tmp/example.bin"),
        data_base64: "AAE=".to_string(),
    };

    let value = serde_json::to_value(&params).expect("serialize fs/writeFile params");
    assert_eq!(
        value,
        json!({
            "path": absolute_path_string("tmp/example.bin"),
            "dataBase64": "AAE=",
        })
    );

    let decoded = serde_json::from_value::<FsWriteFileParams>(value)
        .expect("deserialize fs/writeFile params");
    assert_eq!(decoded, params);
}

#[test]
fn fs_copy_params_round_trip_with_recursive_directory_copy() {
    let params = FsCopyParams {
        source_path: absolute_path("tmp/source"),
        destination_path: absolute_path("tmp/destination"),
        recursive: true,
    };

    let value = serde_json::to_value(&params).expect("serialize fs/copy params");
    assert_eq!(
        value,
        json!({
            "sourcePath": absolute_path_string("tmp/source"),
            "destinationPath": absolute_path_string("tmp/destination"),
            "recursive": true,
        })
    );

    let decoded =
        serde_json::from_value::<FsCopyParams>(value).expect("deserialize fs/copy params");
    assert_eq!(decoded, params);
}

#[test]
fn thread_shell_command_params_round_trip() {
    let params = ThreadShellCommandParams {
        thread_id: "thr_123".to_string(),
        command: "printf 'hello world\\n'".to_string(),
    };

    let value = serde_json::to_value(&params).expect("serialize thread/shellCommand params");
    assert_eq!(
        value,
        json!({
            "threadId": "thr_123",
            "command": "printf 'hello world\\n'",
        })
    );

    let decoded = serde_json::from_value::<ThreadShellCommandParams>(value)
        .expect("deserialize thread/shellCommand params");
    assert_eq!(decoded, params);
}

#[test]
fn thread_shell_command_response_round_trip() {
    let response = ThreadShellCommandResponse {};

    let value = serde_json::to_value(&response).expect("serialize thread/shellCommand response");
    assert_eq!(value, json!({}));

    let decoded = serde_json::from_value::<ThreadShellCommandResponse>(value)
        .expect("deserialize thread/shellCommand response");
    assert_eq!(decoded, response);
}

#[test]
fn fs_changed_notification_round_trips() {
    let notification = FsChangedNotification {
        watch_id: "0195ec6b-1d6f-7c2e-8c7a-56f2c4a8b9d1".to_string(),
        changed_paths: vec![
            absolute_path("tmp/repo/.git/HEAD"),
            absolute_path("tmp/repo/.git/FETCH_HEAD"),
        ],
    };

    let value = serde_json::to_value(&notification).expect("serialize fs/changed notification");
    assert_eq!(
        value,
        json!({
            "watchId": "0195ec6b-1d6f-7c2e-8c7a-56f2c4a8b9d1",
            "changedPaths": [
                absolute_path_string("tmp/repo/.git/HEAD"),
                absolute_path_string("tmp/repo/.git/FETCH_HEAD"),
            ],
        })
    );

    let decoded = serde_json::from_value::<FsChangedNotification>(value)
        .expect("deserialize fs/changed notification");
    assert_eq!(decoded, notification);
}

#[test]
fn command_exec_params_default_optional_streaming_flags() {
    let params = serde_json::from_value::<CommandExecParams>(json!({
        "command": ["ls", "-la"],
        "timeoutMs": 1000,
        "cwd": "/tmp"
    }))
    .expect("command/exec payload should deserialize");

    assert_eq!(
        params,
        CommandExecParams {
            command: vec!["ls".to_string(), "-la".to_string()],
            process_id: None,
            tty: false,
            stream_stdin: false,
            stream_stdout_stderr: false,
            output_bytes_cap: None,
            disable_output_cap: false,
            disable_timeout: false,
            timeout_ms: Some(1000),
            cwd: Some(PathBuf::from("/tmp")),
            env: None,
            size: None,
            sandbox_policy: None,
        }
    );
}

#[test]
fn command_exec_params_round_trips_disable_timeout() {
    let params = CommandExecParams {
        command: vec!["sleep".to_string(), "30".to_string()],
        process_id: Some("sleep-1".to_string()),
        tty: false,
        stream_stdin: false,
        stream_stdout_stderr: false,
        output_bytes_cap: None,
        disable_output_cap: false,
        disable_timeout: true,
        timeout_ms: None,
        cwd: None,
        env: None,
        size: None,
        sandbox_policy: None,
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec params");
    assert_eq!(
        value,
        json!({
            "command": ["sleep", "30"],
            "processId": "sleep-1",
            "disableTimeout": true,
            "timeoutMs": null,
            "cwd": null,
            "env": null,
            "size": null,
            "sandboxPolicy": null,
            "outputBytesCap": null,
        })
    );

    let decoded =
        serde_json::from_value::<CommandExecParams>(value).expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_params_round_trips_disable_output_cap() {
    let params = CommandExecParams {
        command: vec!["yes".to_string()],
        process_id: Some("yes-1".to_string()),
        tty: false,
        stream_stdin: false,
        stream_stdout_stderr: true,
        output_bytes_cap: None,
        disable_output_cap: true,
        disable_timeout: false,
        timeout_ms: None,
        cwd: None,
        env: None,
        size: None,
        sandbox_policy: None,
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec params");
    assert_eq!(
        value,
        json!({
            "command": ["yes"],
            "processId": "yes-1",
            "streamStdoutStderr": true,
            "outputBytesCap": null,
            "disableOutputCap": true,
            "timeoutMs": null,
            "cwd": null,
            "env": null,
            "size": null,
            "sandboxPolicy": null,
        })
    );

    let decoded =
        serde_json::from_value::<CommandExecParams>(value).expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_params_round_trips_env_overrides_and_unsets() {
    let params = CommandExecParams {
        command: vec!["printenv".to_string(), "FOO".to_string()],
        process_id: Some("env-1".to_string()),
        tty: false,
        stream_stdin: false,
        stream_stdout_stderr: false,
        output_bytes_cap: None,
        disable_output_cap: false,
        disable_timeout: false,
        timeout_ms: None,
        cwd: None,
        env: Some(HashMap::from([
            ("FOO".to_string(), Some("override".to_string())),
            ("BAR".to_string(), Some("added".to_string())),
            ("BAZ".to_string(), None),
        ])),
        size: None,
        sandbox_policy: None,
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec params");
    assert_eq!(
        value,
        json!({
            "command": ["printenv", "FOO"],
            "processId": "env-1",
            "outputBytesCap": null,
            "timeoutMs": null,
            "cwd": null,
            "env": {
                "FOO": "override",
                "BAR": "added",
                "BAZ": null,
            },
            "size": null,
            "sandboxPolicy": null,
        })
    );

    let decoded =
        serde_json::from_value::<CommandExecParams>(value).expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_write_round_trips_close_only_payload() {
    let params = CommandExecWriteParams {
        process_id: "proc-7".to_string(),
        delta_base64: None,
        close_stdin: true,
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec/write params");
    assert_eq!(
        value,
        json!({
            "processId": "proc-7",
            "deltaBase64": null,
            "closeStdin": true,
        })
    );

    let decoded =
        serde_json::from_value::<CommandExecWriteParams>(value).expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_terminate_round_trips() {
    let params = CommandExecTerminateParams {
        process_id: "proc-8".to_string(),
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec/terminate params");
    assert_eq!(
        value,
        json!({
            "processId": "proc-8",
        })
    );

    let decoded = serde_json::from_value::<CommandExecTerminateParams>(value)
        .expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_params_round_trip_with_size() {
    let params = CommandExecParams {
        command: vec!["top".to_string()],
        process_id: Some("pty-1".to_string()),
        tty: true,
        stream_stdin: false,
        stream_stdout_stderr: false,
        output_bytes_cap: None,
        disable_output_cap: false,
        disable_timeout: false,
        timeout_ms: None,
        cwd: None,
        env: None,
        size: Some(CommandExecTerminalSize {
            rows: 40,
            cols: 120,
        }),
        sandbox_policy: None,
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec params");
    assert_eq!(
        value,
        json!({
            "command": ["top"],
            "processId": "pty-1",
            "tty": true,
            "outputBytesCap": null,
            "timeoutMs": null,
            "cwd": null,
            "env": null,
            "size": {
                "rows": 40,
                "cols": 120,
            },
            "sandboxPolicy": null,
        })
    );

    let decoded =
        serde_json::from_value::<CommandExecParams>(value).expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_resize_round_trips() {
    let params = CommandExecResizeParams {
        process_id: "proc-9".to_string(),
        size: CommandExecTerminalSize {
            rows: 50,
            cols: 160,
        },
    };

    let value = serde_json::to_value(&params).expect("serialize command/exec/resize params");
    assert_eq!(
        value,
        json!({
            "processId": "proc-9",
            "size": {
                "rows": 50,
                "cols": 160,
            },
        })
    );

    let decoded =
        serde_json::from_value::<CommandExecResizeParams>(value).expect("deserialize round-trip");
    assert_eq!(decoded, params);
}

#[test]
fn command_exec_output_delta_round_trips() {
    let notification = CommandExecOutputDeltaNotification {
        process_id: "proc-1".to_string(),
        stream: CommandExecOutputStream::Stdout,
        delta_base64: "AQI=".to_string(),
        cap_reached: false,
    };

    let value = serde_json::to_value(&notification)
        .expect("serialize command/exec/outputDelta notification");
    assert_eq!(
        value,
        json!({
            "processId": "proc-1",
            "stream": "stdout",
            "deltaBase64": "AQI=",
            "capReached": false,
        })
    );

    let decoded = serde_json::from_value::<CommandExecOutputDeltaNotification>(value)
        .expect("deserialize round-trip");
    assert_eq!(decoded, notification);
}

#[test]
fn command_execution_output_delta_round_trips() {
    let notification = CommandExecutionOutputDeltaNotification {
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        item_id: "item-1".to_string(),
        delta: "\u{fffd}a\n".to_string(),
    };

    let value = serde_json::to_value(&notification)
        .expect("serialize item/commandExecution/outputDelta notification");
    assert_eq!(
        value,
        json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "item-1",
            "delta": "\u{fffd}a\n",
        })
    );

    let decoded = serde_json::from_value::<CommandExecutionOutputDeltaNotification>(value)
        .expect("deserialize round-trip");
    assert_eq!(decoded, notification);
}
