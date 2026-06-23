use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::PathBuf;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use praxis_app_gateway_protocol::McpElicitationEnumSchema;
use praxis_app_gateway_protocol::McpElicitationPrimitiveSchema;
use praxis_app_gateway_protocol::McpElicitationSingleSelectEnumSchema;
use praxis_app_gateway_protocol::McpServerElicitationRequest;
use praxis_app_gateway_protocol::McpServerElicitationRequestParams;
use praxis_protocol::ThreadId;
use praxis_protocol::approvals::ElicitationAction;
use praxis_protocol::approvals::ElicitationRequest;
use praxis_protocol::approvals::ElicitationRequestEvent;
use praxis_protocol::mcp::RequestId as McpRequestId;
#[cfg(test)]
use praxis_protocol::protocol::Op;
use praxis_protocol::user_input::TextElement;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::ChatComposer;
use crate::bottom_pane::ChatComposerConfig;
use crate::bottom_pane::InputResult;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::scroll_state::ScrollState;
use crate::bottom_pane::selection_popup_common::GenericDisplayRow;
use crate::bottom_pane::selection_popup_common::measure_rows_height;
use crate::bottom_pane::selection_popup_common::menu_surface_inset;
use crate::bottom_pane::selection_popup_common::menu_surface_padding_height;
use crate::bottom_pane::selection_popup_common::render_menu_surface;
use crate::bottom_pane::selection_popup_common::render_rows;
use crate::render::renderable::Renderable;
use crate::text_formatting::format_json_compact;
use crate::text_formatting::truncate_text;

const ANSWER_PLACEHOLDER: &str = "Type your answer";
const OPTIONAL_ANSWER_PLACEHOLDER: &str = "Type your answer (optional)";
const FOOTER_SEPARATOR: &str = " | ";
const MIN_COMPOSER_HEIGHT: u16 = 3;
const MIN_OVERLAY_HEIGHT: u16 = 8;
const APPROVAL_FIELD_ID: &str = "__approval";
const APPROVAL_ACCEPT_ONCE_VALUE: &str = "accept";
const APPROVAL_ACCEPT_SESSION_VALUE: &str = "accept_session";
const APPROVAL_ACCEPT_ALWAYS_VALUE: &str = "accept_always";
const APPROVAL_DECLINE_VALUE: &str = "decline";
const APPROVAL_CANCEL_VALUE: &str = "cancel";
const APPROVAL_META_KIND_KEY: &str = "praxis_approval_kind";
const APPROVAL_META_KIND_MCP_TOOL_CALL: &str = "mcp_tool_call";
const APPROVAL_META_KIND_TOOL_SUGGESTION: &str = "tool_suggestion";
const APPROVAL_PERSIST_KEY: &str = "persist";
const APPROVAL_PERSIST_SESSION_VALUE: &str = "session";
const APPROVAL_PERSIST_ALWAYS_VALUE: &str = "always";
const APPROVAL_TOOL_PARAMS_KEY: &str = "tool_params";
const APPROVAL_TOOL_PARAMS_DISPLAY_KEY: &str = "tool_params_display";
const APPROVAL_TOOL_PARAM_DISPLAY_LIMIT: usize = 3;
const APPROVAL_TOOL_PARAM_VALUE_TRUNCATE_GRAPHEMES: usize = 60;
const TOOL_TYPE_KEY: &str = "tool_type";
const TOOL_ID_KEY: &str = "tool_id";
const TOOL_NAME_KEY: &str = "tool_name";
const TOOL_SUGGEST_SUGGEST_TYPE_KEY: &str = "suggest_type";
const TOOL_SUGGEST_REASON_KEY: &str = "suggest_reason";
const TOOL_SUGGEST_INSTALL_URL_KEY: &str = "install_url";

#[derive(Clone, PartialEq, Default)]
struct ComposerDraft {
    text: String,
    text_elements: Vec<TextElement>,
    local_image_paths: Vec<PathBuf>,
    pending_pastes: Vec<(String, String)>,
}

impl ComposerDraft {
    fn text_with_pending(&self) -> String {
        if self.pending_pastes.is_empty() {
            return self.text.clone();
        }
        debug_assert!(
            !self.text_elements.is_empty(),
            "pending pastes should always have matching text elements"
        );
        let (expanded, _) = ChatComposer::expand_pending_pastes(
            &self.text,
            self.text_elements.clone(),
            &self.pending_pastes,
        );
        expanded
    }
}

#[derive(Clone, Debug, PartialEq)]
struct McpServerElicitationOption {
    label: String,
    description: Option<String>,
    value: Value,
}

#[derive(Clone, Debug, PartialEq)]
enum McpServerElicitationFieldInput {
    Select {
        options: Vec<McpServerElicitationOption>,
        default_idx: Option<usize>,
    },
    Text {
        secret: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct McpServerElicitationField {
    id: String,
    label: String,
    prompt: String,
    required: bool,
    input: McpServerElicitationFieldInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum McpServerElicitationResponseMode {
    FormContent,
    ApprovalAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolSuggestionToolType {
    Connector,
    Plugin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolSuggestionType {
    Install,
    Enable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolSuggestionRequest {
    pub(crate) tool_type: ToolSuggestionToolType,
    pub(crate) suggest_type: ToolSuggestionType,
    pub(crate) suggest_reason: String,
    pub(crate) tool_id: String,
    pub(crate) tool_name: String,
    pub(crate) install_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct McpToolApprovalDisplayParam {
    name: String,
    value: Value,
    display_name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct McpServerElicitationFormRequest {
    thread_id: ThreadId,
    server_name: String,
    request_id: McpRequestId,
    message: String,
    approval_display_params: Vec<McpToolApprovalDisplayParam>,
    response_mode: McpServerElicitationResponseMode,
    fields: Vec<McpServerElicitationField>,
    tool_suggestion: Option<ToolSuggestionRequest>,
}

#[derive(Default)]
struct McpServerElicitationAnswerState {
    selection: ScrollState,
    draft: ComposerDraft,
    answer_committed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FooterTip {
    text: String,
    highlight: bool,
}

impl FooterTip {
    fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            highlight: false,
        }
    }

    fn highlighted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            highlight: true,
        }
    }
}

mod overlay;
mod render;
mod request;
mod schema;
mod tool_approval;
mod view;

pub(crate) use self::overlay::McpServerElicitationOverlay;
use self::render::*;
use self::schema::*;
use self::tool_approval::*;
use self::view::*;

#[cfg(test)]
mod tests;
