use super::*;

impl App {
    pub(super) fn start_thread_name_generation(
        &mut self,
        app_gateway: &mut AppGatewaySession,
        thread_id: praxis_protocol::ThreadId,
    ) {
        let locked = self.workspace.rows.iter().any(|row| {
            row.thread_id == thread_id && crate::workspace::workspace_row_is_controlled(row)
        });
        if locked {
            self.chat_widget
                .add_error_message("This thread is controlled by another agent.".to_string());
        } else if self.workspace.name_generation_in_flight.insert(thread_id) {
            self.chat_widget.add_info_message(
                "Regenerating thread name.".to_string(),
                Some(
                    "Praxis will rename this thread from the current conversation context."
                        .to_string(),
                ),
            );
            let request = app_gateway.thread_regenerate_name(thread_id);
            let tx = self.app_event_tx.clone();
            tokio::spawn(async move {
                let result = request.await.map_err(|err| format!("{err:#}"));
                tx.send(AppEvent::ThreadNameRegenerated { thread_id, result });
            });
        }
    }

    pub(super) fn finish_thread_name_generation(
        &mut self,
        thread_id: praxis_protocol::ThreadId,
        result: std::result::Result<String, String>,
    ) {
        self.workspace.name_generation_in_flight.remove(&thread_id);
        match result {
            Ok(thread_name) => {
                if let Some(index) = self.workspace.row_index(thread_id) {
                    self.workspace.rows[index].name = workspace_single_line(&thread_name);
                    self.workspace.mark_subagent_summaries_dirty();
                }
                self.chat_widget.add_info_message(
                    format!("Thread name regenerated: {thread_name}"),
                    /*hint*/ None,
                );
            }
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to regenerate thread name: {err}"));
            }
        }
    }
}
