use std::collections::HashMap;
use std::path::PathBuf;

use crate::exec_command::split_command_string;
use praxis_app_gateway_protocol::AdditionalFileSystemPermissions;
use praxis_app_gateway_protocol::AdditionalNetworkPermissions;
use praxis_app_gateway_protocol::CollabAgentState as AppGatewayCollabAgentState;
use praxis_app_gateway_protocol::CollabAgentStatus as AppGatewayCollabAgentStatus;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalParams;
use praxis_app_gateway_protocol::FileChangeRequestApprovalParams;
use praxis_app_gateway_protocol::FileUpdateChange;
use praxis_app_gateway_protocol::GrantedPermissionProfile;
use praxis_app_gateway_protocol::NetworkApprovalContext as AppGatewayNetworkApprovalContext;
use praxis_app_gateway_protocol::PatchChangeKind;
use praxis_app_gateway_protocol::PermissionsRequestApprovalParams;
use praxis_app_gateway_protocol::RequestId as AppGatewayRequestId;
use praxis_app_gateway_protocol::ToolRequestUserInputParams;
use praxis_app_gateway_protocol::WebSearchAction as AppGatewayWebSearchAction;
use praxis_protocol::mcp::RequestId as McpRequestId;
use praxis_protocol::models::WebSearchAction;
use praxis_protocol::protocol::AgentStatus;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::FileChange;
use praxis_protocol::protocol::NetworkApprovalContext;
use praxis_protocol::protocol::NetworkApprovalProtocol;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsEvent;
use praxis_protocol::request_user_input::RequestUserInputEvent;
use praxis_protocol::request_user_input::RequestUserInputQuestionOption;

pub(crate) fn network_approval_context_to_core(
    value: AppGatewayNetworkApprovalContext,
) -> NetworkApprovalContext {
    NetworkApprovalContext {
        host: value.host,
        protocol: match value.protocol {
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Http => {
                NetworkApprovalProtocol::Http
            }
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Https => {
                NetworkApprovalProtocol::Https
            }
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Socks5Tcp => {
                NetworkApprovalProtocol::Socks5Tcp
            }
            praxis_app_gateway_protocol::NetworkApprovalProtocol::Socks5Udp => {
                NetworkApprovalProtocol::Socks5Udp
            }
        },
    }
}

pub(crate) fn granted_permission_profile_from_request(
    value: CoreRequestPermissionProfile,
) -> GrantedPermissionProfile {
    GrantedPermissionProfile {
        network: value.network.map(|network| AdditionalNetworkPermissions {
            enabled: network.enabled,
        }),
        file_system: value
            .file_system
            .map(|file_system| AdditionalFileSystemPermissions {
                read: file_system.read,
                write: file_system.write,
            }),
    }
}

pub(crate) fn app_gateway_request_id_to_mcp_request_id(
    request_id: &AppGatewayRequestId,
) -> McpRequestId {
    match request_id {
        AppGatewayRequestId::String(value) => McpRequestId::String(value.clone()),
        AppGatewayRequestId::Integer(value) => McpRequestId::Integer(*value),
    }
}

pub(crate) fn command_execution_decision_to_review_decision(
    decision: CommandExecutionApprovalDecision,
) -> ReviewDecision {
    match decision {
        CommandExecutionApprovalDecision::Accept => ReviewDecision::Approved,
        CommandExecutionApprovalDecision::AcceptForSession => ReviewDecision::ApprovedForSession,
        CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment {
            execpolicy_amendment,
        } => ReviewDecision::ApprovedExecpolicyAmendment {
            proposed_execpolicy_amendment: execpolicy_amendment.into_core(),
        },
        CommandExecutionApprovalDecision::ApplyNetworkPolicyAmendment {
            network_policy_amendment,
        } => ReviewDecision::NetworkPolicyAmendment {
            network_policy_amendment: network_policy_amendment.into_core(),
        },
        CommandExecutionApprovalDecision::Decline => ReviewDecision::Denied,
        CommandExecutionApprovalDecision::Cancel => ReviewDecision::Abort,
    }
}

pub(crate) fn exec_approval_request_from_params(
    params: CommandExecutionRequestApprovalParams,
) -> ExecApprovalRequestEvent {
    ExecApprovalRequestEvent {
        call_id: params.item_id,
        command: params
            .command
            .as_deref()
            .map(split_command_string)
            .unwrap_or_default(),
        cwd: params.cwd.unwrap_or_default(),
        reason: params.reason,
        network_approval_context: params
            .network_approval_context
            .map(network_approval_context_to_core),
        additional_permissions: params.additional_permissions.map(Into::into),
        turn_id: params.turn_id,
        approval_id: params.approval_id,
        proposed_execpolicy_amendment: params
            .proposed_execpolicy_amendment
            .map(praxis_app_gateway_protocol::ExecPolicyAmendment::into_core),
        proposed_network_policy_amendments: params.proposed_network_policy_amendments.map(
            |amendments| {
                amendments
                    .into_iter()
                    .map(praxis_app_gateway_protocol::NetworkPolicyAmendment::into_core)
                    .collect()
            },
        ),
        available_decisions: params.available_decisions.map(|decisions| {
            decisions
                .into_iter()
                .map(command_execution_decision_to_review_decision)
                .collect()
        }),
        parsed_cmd: params
            .command_actions
            .unwrap_or_default()
            .into_iter()
            .map(praxis_app_gateway_protocol::CommandAction::into_core)
            .collect(),
    }
}

pub(crate) fn patch_approval_request_from_params(
    params: FileChangeRequestApprovalParams,
) -> ApplyPatchApprovalRequestEvent {
    ApplyPatchApprovalRequestEvent {
        call_id: params.item_id,
        turn_id: params.turn_id,
        changes: HashMap::new(),
        reason: params.reason,
        grant_root: params.grant_root,
    }
}

pub(crate) fn app_gateway_patch_changes_to_core(
    changes: Vec<FileUpdateChange>,
) -> HashMap<PathBuf, FileChange> {
    changes
        .into_iter()
        .map(|change| {
            let path = PathBuf::from(change.path);
            let file_change = match change.kind {
                PatchChangeKind::Add => FileChange::Add {
                    content: change.diff,
                },
                PatchChangeKind::Delete => FileChange::Delete {
                    content: change.diff,
                },
                PatchChangeKind::Update { move_path } => FileChange::Update {
                    unified_diff: change.diff,
                    move_path,
                },
            };
            (path, file_change)
        })
        .collect()
}

pub(crate) fn app_gateway_collab_thread_id_to_core(
    thread_id: &str,
) -> Option<praxis_protocol::ThreadId> {
    match praxis_protocol::ThreadId::from_string(thread_id) {
        Ok(thread_id) => Some(thread_id),
        Err(err) => {
            tracing::warn!(
                "ignoring collab tool-call item with invalid thread id {thread_id}: {err}"
            );
            None
        }
    }
}

pub(crate) fn app_gateway_collab_state_to_core(state: &AppGatewayCollabAgentState) -> AgentStatus {
    match state.status {
        AppGatewayCollabAgentStatus::PendingInit => AgentStatus::PendingInit,
        AppGatewayCollabAgentStatus::Running => AgentStatus::Running,
        AppGatewayCollabAgentStatus::Interrupted => AgentStatus::Interrupted,
        AppGatewayCollabAgentStatus::Completed => AgentStatus::Completed(state.message.clone()),
        AppGatewayCollabAgentStatus::Errored => AgentStatus::Errored(
            state
                .message
                .clone()
                .unwrap_or_else(|| "Agent errored".into()),
        ),
        AppGatewayCollabAgentStatus::Shutdown => AgentStatus::Shutdown,
        AppGatewayCollabAgentStatus::NotFound => AgentStatus::NotFound,
    }
}

pub(crate) fn request_permissions_from_params(
    params: PermissionsRequestApprovalParams,
) -> RequestPermissionsEvent {
    RequestPermissionsEvent {
        turn_id: params.turn_id,
        call_id: params.item_id,
        reason: params.reason,
        permissions: params.permissions.into(),
    }
}

pub(crate) fn request_user_input_from_params(
    params: ToolRequestUserInputParams,
) -> RequestUserInputEvent {
    RequestUserInputEvent {
        turn_id: params.turn_id,
        call_id: params.item_id,
        questions: params
            .questions
            .into_iter()
            .map(
                |question| praxis_protocol::request_user_input::RequestUserInputQuestion {
                    id: question.id,
                    header: question.header,
                    question: question.question,
                    is_other: question.is_other,
                    is_secret: question.is_secret,
                    options: question.options.map(|options| {
                        options
                            .into_iter()
                            .map(|option| RequestUserInputQuestionOption {
                                label: option.label,
                                description: option.description,
                            })
                            .collect()
                    }),
                },
            )
            .collect(),
    }
}

pub(crate) fn app_gateway_web_search_action_to_core(
    action: AppGatewayWebSearchAction,
) -> WebSearchAction {
    match action {
        AppGatewayWebSearchAction::Search { query, queries } => {
            WebSearchAction::Search { query, queries }
        }
        AppGatewayWebSearchAction::OpenPage { url } => WebSearchAction::OpenPage { url },
        AppGatewayWebSearchAction::FindInPage { url, pattern } => {
            WebSearchAction::FindInPage { url, pattern }
        }
        AppGatewayWebSearchAction::Other => WebSearchAction::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::app_gateway_patch_changes_to_core;
    use super::app_gateway_request_id_to_mcp_request_id;
    use super::granted_permission_profile_from_request;
    use super::network_approval_context_to_core;
    use praxis_app_gateway_protocol::FileUpdateChange;
    use praxis_app_gateway_protocol::PatchChangeKind;
    use praxis_app_gateway_protocol::RequestId as AppGatewayRequestId;
    use praxis_protocol::mcp::RequestId as McpRequestId;
    use praxis_protocol::models::FileSystemPermissions;
    use praxis_protocol::models::NetworkPermissions;
    use praxis_protocol::protocol::FileChange;
    use praxis_protocol::protocol::NetworkApprovalContext;
    use praxis_protocol::protocol::NetworkApprovalProtocol;
    use praxis_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
    use praxis_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    fn absolute_path(path: &str) -> AbsolutePathBuf {
        AbsolutePathBuf::try_from(PathBuf::from(path)).expect("path must be absolute")
    }

    #[test]
    fn converts_app_gateway_network_approval_context_to_core() {
        assert_eq!(
            network_approval_context_to_core(praxis_app_gateway_protocol::NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: praxis_app_gateway_protocol::NetworkApprovalProtocol::Socks5Tcp,
            }),
            NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: NetworkApprovalProtocol::Socks5Tcp,
            }
        );
    }

    #[test]
    fn converts_app_gateway_request_id_to_mcp_request_id() {
        assert_eq!(
            app_gateway_request_id_to_mcp_request_id(&AppGatewayRequestId::String(
                "req-1".to_string()
            )),
            McpRequestId::String("req-1".to_string())
        );
        assert_eq!(
            app_gateway_request_id_to_mcp_request_id(&AppGatewayRequestId::Integer(7)),
            McpRequestId::Integer(7)
        );
    }

    #[test]
    fn converts_request_permissions_into_granted_permissions() {
        assert_eq!(
            granted_permission_profile_from_request(CoreRequestPermissionProfile {
                network: Some(NetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(FileSystemPermissions {
                    read: Some(vec![absolute_path("/tmp/read-only")]),
                    write: Some(vec![absolute_path("/tmp/write")]),
                }),
            }),
            praxis_app_gateway_protocol::GrantedPermissionProfile {
                network: Some(praxis_app_gateway_protocol::AdditionalNetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(
                    praxis_app_gateway_protocol::AdditionalFileSystemPermissions {
                        read: Some(vec![absolute_path("/tmp/read-only")]),
                        write: Some(vec![absolute_path("/tmp/write")]),
                    }
                ),
            }
        );
    }

    #[test]
    fn converts_app_gateway_patch_changes_to_core() {
        let changes = app_gateway_patch_changes_to_core(vec![FileUpdateChange {
            path: "src/main.rs".to_string(),
            kind: PatchChangeKind::Update {
                move_path: Some(PathBuf::from("src/lib.rs")),
            },
            diff: "@@ -1 +1 @@".to_string(),
        }]);

        assert_eq!(
            changes.get(&PathBuf::from("src/main.rs")),
            Some(&FileChange::Update {
                unified_diff: "@@ -1 +1 @@".to_string(),
                move_path: Some(PathBuf::from("src/lib.rs")),
            })
        );
    }
}
