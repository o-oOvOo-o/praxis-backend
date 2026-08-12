use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::CommandExecutionApprovalDecision;
use crate::CommandExecutionRequestApprovalParams;
use crate::CommandExecutionRequestApprovalResponse;
use crate::FileChangeApprovalDecision;
use crate::FileChangeRequestApprovalResponse;
use crate::GrantedPermissionProfile;
use crate::McpServerElicitationAction;
use crate::McpServerElicitationRequest;
use crate::McpServerElicitationRequestParams;
use crate::McpServerElicitationRequestResponse;
use crate::PermissionGrantScope;
use crate::PermissionsRequestApprovalParams;
use crate::PermissionsRequestApprovalResponse;
use crate::ServerRequest;

const MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX: &str = "mcp_tool_call_approval_";
const MCP_TOOL_APPROVAL_KIND_KEY: &str = "praxis_approval_kind";
const MCP_TOOL_APPROVAL_KIND: &str = "mcp_tool_call";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRequestKind {
    Command,
    FileChange,
    Permissions,
    McpElicitation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalResponseAction {
    Accept,
    AcceptForSession,
    AcceptExecpolicyAmendment,
    ApplyNetworkPolicyAmendment,
    Decline,
    Cancel,
}

pub fn is_approval_server_request(request: &ServerRequest) -> bool {
    match request {
        ServerRequest::CommandExecutionRequestApproval { .. }
        | ServerRequest::FileChangeRequestApproval { .. }
        | ServerRequest::PermissionsRequestApproval { .. } => true,
        ServerRequest::ToolRequestUserInput { params, .. } => {
            is_mcp_tool_approval_user_input(&params.questions)
        }
        ServerRequest::McpServerElicitationRequest { params, .. } => {
            is_mcp_tool_approval_elicitation(&params.request)
        }
        _ => false,
    }
}

pub fn is_mcp_tool_approval_user_input(questions: &[crate::ToolRequestUserInputQuestion]) -> bool {
    questions.iter().any(|question| {
        question
            .id
            .starts_with(MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX)
    })
}

pub fn is_mcp_tool_approval_elicitation(request: &McpServerElicitationRequest) -> bool {
    let meta = match request {
        McpServerElicitationRequest::Form { meta, .. }
        | McpServerElicitationRequest::Url { meta, .. } => meta.as_ref(),
    };
    meta.and_then(Value::as_object)
        .and_then(|meta| meta.get(MCP_TOOL_APPROVAL_KIND_KEY))
        .and_then(Value::as_str)
        == Some(MCP_TOOL_APPROVAL_KIND)
}

pub fn command_approval_decisions_from_params(
    raw_params: &Value,
) -> Vec<CommandExecutionApprovalDecision> {
    serde_json::from_value::<CommandExecutionRequestApprovalParams>(raw_params.clone())
        .map(|params| params.effective_available_decisions())
        .unwrap_or_default()
}

pub fn approval_response_for_action(
    kind: ApprovalRequestKind,
    raw_params: &Value,
    action: ApprovalResponseAction,
    permission_scope: Option<PermissionGrantScope>,
) -> Option<Value> {
    match kind {
        ApprovalRequestKind::Command => {
            let decision = command_approval_decision_for_action(raw_params, action)?;
            serde_json::to_value(CommandExecutionRequestApprovalResponse { decision }).ok()
        }
        ApprovalRequestKind::FileChange => {
            let decision = file_change_approval_decision_for_action(action)?;
            serde_json::to_value(FileChangeRequestApprovalResponse { decision }).ok()
        }
        ApprovalRequestKind::Permissions => {
            permissions_approval_response(raw_params, action, permission_scope)
        }
        ApprovalRequestKind::McpElicitation => {
            let response = mcp_elicitation_response_for_action(raw_params, action)?;
            serde_json::to_value(response).ok()
        }
    }
}

pub const fn default_reject_action(kind: ApprovalRequestKind) -> ApprovalResponseAction {
    match kind {
        ApprovalRequestKind::FileChange => ApprovalResponseAction::Cancel,
        ApprovalRequestKind::Command
        | ApprovalRequestKind::Permissions
        | ApprovalRequestKind::McpElicitation => ApprovalResponseAction::Decline,
    }
}

fn command_approval_decision_for_action(
    raw_params: &Value,
    action: ApprovalResponseAction,
) -> Option<CommandExecutionApprovalDecision> {
    command_approval_decisions_from_params(raw_params)
        .into_iter()
        .find(|decision| match action {
            ApprovalResponseAction::Accept => {
                matches!(decision, CommandExecutionApprovalDecision::Accept)
            }
            ApprovalResponseAction::AcceptForSession => {
                matches!(decision, CommandExecutionApprovalDecision::AcceptForSession)
            }
            ApprovalResponseAction::AcceptExecpolicyAmendment => matches!(
                decision,
                CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment { .. }
            ),
            ApprovalResponseAction::ApplyNetworkPolicyAmendment => matches!(
                decision,
                CommandExecutionApprovalDecision::ApplyNetworkPolicyAmendment { .. }
            ),
            ApprovalResponseAction::Decline => {
                matches!(decision, CommandExecutionApprovalDecision::Decline)
            }
            ApprovalResponseAction::Cancel => {
                matches!(decision, CommandExecutionApprovalDecision::Cancel)
            }
        })
}

fn file_change_approval_decision_for_action(
    action: ApprovalResponseAction,
) -> Option<FileChangeApprovalDecision> {
    match action {
        ApprovalResponseAction::Accept => Some(FileChangeApprovalDecision::Accept),
        ApprovalResponseAction::AcceptForSession => {
            Some(FileChangeApprovalDecision::AcceptForSession)
        }
        ApprovalResponseAction::Decline => Some(FileChangeApprovalDecision::Decline),
        ApprovalResponseAction::Cancel => Some(FileChangeApprovalDecision::Cancel),
        ApprovalResponseAction::AcceptExecpolicyAmendment
        | ApprovalResponseAction::ApplyNetworkPolicyAmendment => None,
    }
}

fn permissions_approval_response(
    raw_params: &Value,
    action: ApprovalResponseAction,
    permission_scope: Option<PermissionGrantScope>,
) -> Option<Value> {
    let (permissions, scope) = match action {
        ApprovalResponseAction::Accept => {
            let params =
                serde_json::from_value::<PermissionsRequestApprovalParams>(raw_params.clone())
                    .ok()?;
            (
                GrantedPermissionProfile {
                    network: params.permissions.network,
                    file_system: params.permissions.file_system,
                },
                permission_scope?,
            )
        }
        ApprovalResponseAction::Decline | ApprovalResponseAction::Cancel => (
            GrantedPermissionProfile {
                network: None,
                file_system: None,
            },
            PermissionGrantScope::Turn,
        ),
        ApprovalResponseAction::AcceptForSession
        | ApprovalResponseAction::AcceptExecpolicyAmendment
        | ApprovalResponseAction::ApplyNetworkPolicyAmendment => return None,
    };
    serde_json::to_value(PermissionsRequestApprovalResponse { permissions, scope }).ok()
}

fn mcp_elicitation_response_for_action(
    raw_params: &Value,
    action: ApprovalResponseAction,
) -> Option<McpServerElicitationRequestResponse> {
    let action = match action {
        ApprovalResponseAction::Accept => McpServerElicitationAction::Accept,
        ApprovalResponseAction::Decline => McpServerElicitationAction::Decline,
        ApprovalResponseAction::Cancel => McpServerElicitationAction::Cancel,
        ApprovalResponseAction::AcceptForSession
        | ApprovalResponseAction::AcceptExecpolicyAmendment
        | ApprovalResponseAction::ApplyNetworkPolicyAmendment => return None,
    };
    let content = if action == McpServerElicitationAction::Accept {
        let params =
            serde_json::from_value::<McpServerElicitationRequestParams>(raw_params.clone()).ok()?;
        match params.request {
            McpServerElicitationRequest::Url { .. } => None,
            McpServerElicitationRequest::Form {
                requested_schema, ..
            } if requested_schema.properties.is_empty() => {
                Some(Value::Object(serde_json::Map::new()))
            }
            McpServerElicitationRequest::Form { .. } => return None,
        }
    } else {
        None
    };
    Some(McpServerElicitationRequestResponse {
        action,
        content,
        meta: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolRequestUserInputQuestion;
    use praxis_protocol::mcp_elicitation::McpElicitationObjectType;
    use praxis_protocol::mcp_elicitation::McpElicitationSchema;
    use std::collections::BTreeMap;

    #[test]
    fn classifies_only_mcp_approval_user_input() {
        let question = |id: &str| ToolRequestUserInputQuestion {
            id: id.to_string(),
            header: "header".to_string(),
            question: "question".to_string(),
            is_other: false,
            is_secret: false,
            options: None,
        };

        assert!(is_mcp_tool_approval_user_input(&[question(
            "mcp_tool_call_approval_call-1",
        )]));
        assert!(!is_mcp_tool_approval_user_input(&[question(
            "ordinary-question",
        )]));
    }

    #[test]
    fn classifies_mcp_approval_elicitation_from_metadata() {
        let request = McpServerElicitationRequest::Form {
            meta: Some(serde_json::json!({
                "praxis_approval_kind": "mcp_tool_call",
            })),
            message: "approve".to_string(),
            requested_schema: McpElicitationSchema {
                schema_uri: None,
                type_: McpElicitationObjectType::Object,
                properties: BTreeMap::new(),
                required: None,
            },
        };

        assert!(is_mcp_tool_approval_elicitation(&request));
    }
}
