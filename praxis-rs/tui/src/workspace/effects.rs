use super::session_picker::SessionPickerPageRequest;
use crate::resume_picker::SessionSelection;
use praxis_protocol::ThreadId;

// Workspace main-pane effects.
//
// Pickers emit local effects. WorkspaceState resolves those into this flat
// app-facing effect so `app.rs` does not need to know which picker produced
// a selected session, page load, or agent thread.
#[derive(Debug, Clone)]
pub(crate) enum WorkspaceMainPaneEffect {
    None,
    Gateway(WorkspaceGatewayEffect),
}

#[derive(Debug, Clone)]
pub(crate) enum WorkspaceGatewayEffect {
    LoadSessionPickerPage(SessionPickerPageRequest),
    SelectSession(SessionSelection),
    SelectAgent(ThreadId),
}

impl WorkspaceMainPaneEffect {
    pub(crate) fn load_session_picker_page(request: SessionPickerPageRequest) -> Self {
        Self::Gateway(WorkspaceGatewayEffect::LoadSessionPickerPage(request))
    }

    pub(crate) fn select_session(selection: SessionSelection) -> Self {
        Self::Gateway(WorkspaceGatewayEffect::SelectSession(selection))
    }

    pub(crate) fn select_agent(thread_id: ThreadId) -> Self {
        Self::Gateway(WorkspaceGatewayEffect::SelectAgent(thread_id))
    }

    pub(crate) fn error_context(&self) -> &'static str {
        match self {
            Self::None => "Workspace pane",
            Self::Gateway(effect) => effect.error_context(),
        }
    }

    pub(crate) fn into_gateway_effect(self) -> Option<WorkspaceGatewayEffect> {
        match self {
            Self::None => None,
            Self::Gateway(effect) => Some(effect),
        }
    }
}

impl WorkspaceGatewayEffect {
    pub(crate) fn error_context(&self) -> &'static str {
        match self {
            Self::LoadSessionPickerPage(_) | Self::SelectSession(_) => "Session picker",
            Self::SelectAgent(_) => "Agent picker",
        }
    }

    pub(crate) fn schedules_frame_after_apply(&self) -> bool {
        matches!(self, Self::LoadSessionPickerPage(_))
    }
}
