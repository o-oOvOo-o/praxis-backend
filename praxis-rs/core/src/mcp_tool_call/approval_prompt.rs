use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_protocol::request_user_input::RequestUserInputQuestion;
use praxis_protocol::request_user_input::RequestUserInputQuestionOption;

use super::approval_state::McpToolApprovalKey;

pub(crate) const MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX: &str = "mcp_tool_call_approval";
pub(crate) const MCP_TOOL_APPROVAL_ACCEPT: &str = "Allow";
pub(crate) const MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION: &str = "Allow for this session";
pub(crate) const MCP_TOOL_APPROVAL_DECLINE_SYNTHETIC: &str = "__praxis_mcp_decline__";
pub(super) const MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER: &str = "Allow and don't ask me again";
pub(super) const MCP_TOOL_APPROVAL_CANCEL: &str = "Cancel";
pub(super) const MCP_TOOL_APPROVAL_KIND_KEY: &str = "praxis_approval_kind";
pub(super) const MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL: &str = "mcp_tool_call";
pub(super) const MCP_TOOL_APPROVAL_PERSIST_KEY: &str = "persist";
pub(super) const MCP_TOOL_APPROVAL_PERSIST_SESSION: &str = "session";
pub(super) const MCP_TOOL_APPROVAL_PERSIST_ALWAYS: &str = "always";
pub(super) const MCP_TOOL_APPROVAL_SOURCE_KEY: &str = "source";
pub(super) const MCP_TOOL_APPROVAL_SOURCE_CONNECTOR: &str = "connector";
pub(super) const MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY: &str = "connector_id";
pub(super) const MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY: &str = "connector_name";
pub(super) const MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY: &str = "connector_description";
pub(super) const MCP_TOOL_APPROVAL_TOOL_TITLE_KEY: &str = "tool_title";
pub(super) const MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY: &str = "tool_description";
pub(super) const MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY: &str = "tool_params";
pub(super) const MCP_TOOL_APPROVAL_TOOL_PARAMS_DISPLAY_KEY: &str = "tool_params_display";

#[derive(Clone, Copy)]
pub(super) struct McpToolApprovalPromptOptions {
    pub(super) allow_session_remember: bool,
    pub(super) allow_persistent_approval: bool,
}

pub(crate) fn is_mcp_tool_approval_question_id(question_id: &str) -> bool {
    question_id
        .strip_prefix(MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX)
        .is_some_and(|suffix| suffix.starts_with('_'))
}

pub(super) fn mcp_tool_approval_prompt_options(
    session_approval_key: Option<&McpToolApprovalKey>,
    persistent_approval_key: Option<&McpToolApprovalKey>,
    tool_call_mcp_elicitation_enabled: bool,
) -> McpToolApprovalPromptOptions {
    McpToolApprovalPromptOptions {
        allow_session_remember: session_approval_key.is_some(),
        allow_persistent_approval: tool_call_mcp_elicitation_enabled
            && persistent_approval_key.is_some(),
    }
}

pub(super) fn build_mcp_tool_approval_question(
    question_id: String,
    server: &str,
    tool_name: &str,
    connector_name: Option<&str>,
    prompt_options: McpToolApprovalPromptOptions,
    question_override: Option<&str>,
) -> RequestUserInputQuestion {
    let question = question_override
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            build_mcp_tool_approval_fallback_message(server, tool_name, connector_name)
        });
    let question = format!("{}?", question.trim_end_matches('?'));

    let mut options = vec![RequestUserInputQuestionOption {
        label: MCP_TOOL_APPROVAL_ACCEPT.to_string(),
        description: "Run the tool and continue.".to_string(),
    }];
    if prompt_options.allow_session_remember {
        options.push(RequestUserInputQuestionOption {
            label: MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION.to_string(),
            description: "Run the tool and remember this choice for this session.".to_string(),
        });
    }
    if prompt_options.allow_persistent_approval {
        options.push(RequestUserInputQuestionOption {
            label: MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER.to_string(),
            description: "Run the tool and remember this choice for future tool calls.".to_string(),
        });
    }
    options.push(RequestUserInputQuestionOption {
        label: MCP_TOOL_APPROVAL_CANCEL.to_string(),
        description: "Cancel this tool call.".to_string(),
    });

    RequestUserInputQuestion {
        id: question_id,
        header: "Approve app tool call?".to_string(),
        question,
        is_other: false,
        is_secret: false,
        options: Some(options),
    }
}

fn build_mcp_tool_approval_fallback_message(
    server: &str,
    tool_name: &str,
    connector_name: Option<&str>,
) -> String {
    let actor = connector_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            if server == PRAXIS_APPS_MCP_SERVER_NAME {
                "this app".to_string()
            } else {
                format!("the {server} MCP server")
            }
        });
    format!("Allow {actor} to run tool \"{tool_name}\"?")
}

pub(super) fn mcp_tool_approval_question_text(
    question: String,
    monitor_reason: Option<&str>,
) -> String {
    match monitor_reason.map(str::trim) {
        Some(reason) if !reason.is_empty() => {
            format!("Tool call needs your approval. Reason: {reason}")
        }
        _ => question,
    }
}
