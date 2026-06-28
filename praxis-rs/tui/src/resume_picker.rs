use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::SessionLookupSource;
use crate::app_gateway_session::AppGatewaySession;
use crate::diff_render::display_path_for;
use crate::key_hint;
use crate::text_formatting::truncate_text;
use crate::thread_pagination::ThreadArchiveFilter;
use crate::thread_pagination::ThreadListPagination;
use crate::thread_pagination::interactive_thread_source_kinds;
use crate::thread_pagination::thread_list_params_with_archive_filter as common_thread_list_params;
use crate::tui::FrameRequester;
use crate::tui::Tui;
use crate::tui::TuiEvent;
use chrono::DateTime;
use chrono::Utc;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadSortKey as AppGatewayThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind;
use praxis_core::ThreadSortKey;
use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize as _;
use ratatui::text::Line;
use ratatui::text::Span;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::warn;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct SessionTarget {
    pub path: Option<PathBuf>,
    pub thread_id: ThreadId,
    pub thread_name: Option<String>,
    pub cwd: Option<PathBuf>,
}

impl SessionTarget {
    pub fn display_label(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| format!("thread {}", self.thread_id))
    }
}

#[derive(Debug, Clone)]
pub enum SessionSelection {
    StartFresh,
    Resume(SessionTarget),
    Fork(SessionTarget),
    Exit,
}

#[derive(Clone, Copy, Debug)]
pub enum SessionPickerAction {
    Resume,
    Fork,
}

impl SessionPickerAction {
    fn title(self) -> &'static str {
        match self {
            SessionPickerAction::Resume => "Resume a previous session",
            SessionPickerAction::Fork => "Fork a previous session",
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            SessionPickerAction::Resume => "resume",
            SessionPickerAction::Fork => "fork",
        }
    }

    pub(crate) fn selection(
        self,
        path: Option<PathBuf>,
        thread_id: ThreadId,
        thread_name: Option<String>,
        cwd: Option<PathBuf>,
    ) -> SessionSelection {
        let target_session = SessionTarget {
            path,
            thread_id,
            thread_name,
            cwd,
        };
        match self {
            SessionPickerAction::Resume => SessionSelection::Resume(target_session),
            SessionPickerAction::Fork => SessionSelection::Fork(target_session),
        }
    }
}

mod render;
mod runner;
mod source;
mod state;

pub use self::runner::run_fork_picker_with_app_gateway;
pub use self::runner::run_resume_picker_with_app_gateway;
pub(crate) use self::source::AlternatePickerSource;

use self::render::*;
use self::source::*;
use self::state::*;

#[cfg(test)]
mod tests;
