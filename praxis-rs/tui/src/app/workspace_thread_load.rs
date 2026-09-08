use super::*;
use crate::app_gateway_session::attach_workspace_thread_with_handle;
use praxis_app_gateway_protocol::ThreadResumeResponse;

#[derive(Debug)]
pub(crate) struct WorkspaceThreadAttached {
    request_id: uuid::Uuid,
    target: SessionTarget,
    config: Config,
    tui_config: TuiRuntimeConfig,
    result: std::result::Result<ThreadResumeResponse, String>,
}

impl App {
    pub(super) fn begin_workspace_thread_attach(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &AppGatewaySession,
        target: SessionTarget,
        config: Config,
        tui_config: TuiRuntimeConfig,
    ) {
        let request_id = uuid::Uuid::new_v4();
        let origin = self.workspace_active_thread_id();
        self.workspace
            .pending_thread_load
            .begin(request_id, target.thread_id, origin);
        let handle = app_gateway.request_handle();
        let tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = attach_workspace_thread_with_handle(handle, target.thread_id)
                .await
                .map_err(|error| format!("{error:#}"));
            tx.send(AppEvent::WorkspaceThreadAttached(Box::new(
                WorkspaceThreadAttached {
                    request_id,
                    target,
                    config,
                    tui_config,
                    result,
                },
            )));
        });
        tui.frame_requester().schedule_frame();
    }

    pub(super) async fn finish_workspace_thread_attach(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        completed: WorkspaceThreadAttached,
    ) -> Result<()> {
        let current = self.workspace_active_thread_id();
        if !self.workspace.pending_thread_load.accept(
            &completed.request_id,
            &completed.target.thread_id,
            current.as_ref(),
        ) || !self.workspace.enabled
        {
            tracing::info!(target: "praxis_tui::thread_switch", thread_id = %completed.target.thread_id, "discarded stale thread attachment");
            return Ok(());
        }
        let WorkspaceThreadAttached {
            target,
            mut config,
            tui_config,
            result,
            ..
        } = completed;
        self.apply_runtime_policy_overrides(&mut config);
        let resumed = match result {
            Ok(response) => app_gateway.finish_thread_attach(&config, response).await,
            Err(error) => Err(color_eyre::eyre::eyre!(error)),
        };
        let resumed = match resumed {
            Ok(resumed) => resumed,
            Err(error) => {
                self.chat_widget.add_error_message(format!(
                    "Failed to open Praxis thread from {}: {error}",
                    target.display_label()
                ));
                tui.frame_requester().schedule_frame();
                return Ok(());
            }
        };
        self.config = config;
        self.tui_config = tui_config;
        tui.set_notification_method(self.tui_config.notification_method);
        self.file_search
            .update_search_dir(self.config.cwd.to_path_buf());
        if let Err(error) = self
            .switch_to_app_gateway_thread_preserving_background(tui, app_gateway, resumed)
            .await
        {
            self.chat_widget.add_error_message(format!(
                "Failed to switch Praxis thread without stopping background work: {error}"
            ));
        }
        self.refresh_workspace_threads(app_gateway, true);
        tui.frame_requester().schedule_frame();
        Ok(())
    }
}
