use super::App;
use crate::app_event::AppEvent;
use praxis_protocol::ThreadId;
use std::collections::VecDeque;

pub(super) struct ThreadSwitchProbe {
    pending: VecDeque<ThreadId>,
    requested: usize,
    pub(super) waiting_for_row: Option<ThreadId>,
}

impl App {
    pub(super) async fn configure_thread_switch_probe(&mut self) -> color_eyre::eyre::Result<()> {
        let Some(path) = std::env::var_os("PRAXIS_TUI_THREAD_SWITCH_PROBE") else {
            return Ok(());
        };
        if !self.workspace.enabled {
            return Err(color_eyre::eyre::eyre!(
                "thread switch probe requires workspace mode"
            ));
        }
        let bytes = tokio::fs::read(path).await?;
        let targets: Vec<ThreadId> = serde_json::from_slice(&bytes)?;
        if targets.is_empty()
            || targets.len() > 4096
            || targets.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(color_eyre::eyre::eyre!(
                "thread switch probe requires 1..4096 targets without adjacent duplicates"
            ));
        }
        self.thread_view_cache.switch_probe = Some(ThreadSwitchProbe {
            pending: targets.into(),
            requested: 0,
            waiting_for_row: None,
        });
        Ok(())
    }

    pub(super) fn start_thread_switch_probe_if_ready(&mut self) {
        if self.chat_widget.thread_id().is_some()
            && self.workspace.last_refresh_at.is_some()
            && !self.workspace.refresh_in_flight
            && self
                .thread_view_cache
                .switch_probe
                .as_ref()
                .is_some_and(|probe| probe.requested == 0)
        {
            self.advance_thread_switch_probe();
        }
    }

    pub(super) async fn select_probe_workspace_thread(
        &mut self,
        tui: &mut crate::tui::Tui,
        app_gateway: &mut crate::app_gateway_session::AppGatewaySession,
        thread_id: ThreadId,
    ) -> color_eyre::eyre::Result<Option<super::AppRunControl>> {
        let Some(row) = self
            .workspace
            .rows
            .iter()
            .find(|row| row.thread_id == thread_id)
        else {
            if !self.workspace.refresh_in_flight && !self.workspace.pagination.has_next_page() {
                return Err(color_eyre::eyre::eyre!(
                    "probe target absent after all workspace pages: {thread_id}"
                ));
            }
            if let Some(probe) = self.thread_view_cache.switch_probe.as_mut() {
                probe.waiting_for_row = Some(thread_id);
            }
            self.load_more_workspace_threads(app_gateway);
            return Ok(None);
        };
        let target = crate::resume_picker::SessionTarget {
            path: row.path.clone(),
            thread_id,
            thread_name: Some(row.name.clone()),
            cwd: Some(row.cwd.clone()),
        };
        self.resume_session_target(tui, app_gateway, target).await
    }

    pub(super) fn advance_thread_switch_probe(&mut self) {
        let Some(probe) = self.thread_view_cache.switch_probe.as_mut() else {
            return;
        };
        while let Some(target) = probe.pending.pop_front() {
            if self.active_thread_id == Some(target) {
                continue;
            }
            probe.requested += 1;
            tracing::info!(target: "praxis_tui::thread_switch", thread_id = %target,
                ordinal = probe.requested, "thread switch probe requested");
            self.app_event_tx
                .send(AppEvent::ProbeSelectWorkspaceThread(target));
            return;
        }
        tracing::info!(target: "praxis_tui::thread_switch", requested = probe.requested, "thread switch probe complete");
        self.thread_view_cache.switch_probe = None;
    }
}
