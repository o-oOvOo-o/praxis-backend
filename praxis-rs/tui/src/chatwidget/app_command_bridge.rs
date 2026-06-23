use super::*;

impl ChatWidget {
    /// Forward a command directly to the application command pipeline.
    pub(crate) fn submit_op<T>(&mut self, op: T) -> bool
    where
        T: Into<AppCommand>,
    {
        let op: AppCommand = op.into();
        if op.is_review() && !self.bottom_pane.is_task_running() {
            self.bottom_pane.set_task_running(/*running*/ true);
        }
        match &self.praxis_op_target {
            PraxisOpTarget::Direct(praxis_op_tx) => {
                crate::session_log::log_outbound_op(&op);
                if let Err(e) = praxis_op_tx.send(op.into_core()) {
                    tracing::error!("failed to submit op: {e}");
                    return false;
                }
            }
            PraxisOpTarget::AppEvent => {
                self.app_event_tx.send(AppEvent::AgentOp(op.into()));
            }
        }
        true
    }

    #[cfg(test)]
    pub(super) fn on_list_mcp_tools(&mut self, ev: McpListToolsResponseEvent) {
        self.add_to_history(history_cell::new_mcp_tools_output(
            &self.config,
            ev.tools,
            ev.resources,
            ev.resource_templates,
            &ev.auth_statuses,
        ));
    }

    pub(super) fn on_list_skills(&mut self, ev: ListSkillsResponseEvent) {
        self.set_skills_from_response(&ev);
        self.refresh_plugin_mentions();
    }
}
