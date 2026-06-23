use super::*;

impl ChatWidget {
    /// Stores or overwrites the cached nickname and role for a collab agent thread.
    ///
    /// Called by `App::upsert_agent_picker_thread` and `App::replace_chat_widget` to keep the
    /// rendering metadata in sync with the navigation cache. Must be called before any
    /// notification referencing this thread is processed, otherwise the rendered item will fall
    /// back to showing the raw thread id.
    pub(crate) fn set_collab_agent_metadata(
        &mut self,
        thread_id: ThreadId,
        agent_base_name: Option<String>,
        agent_title: Option<String>,
        agent_display_name: Option<String>,
        agent_role: Option<String>,
    ) {
        self.collab_agent_metadata.insert(
            thread_id,
            CollabAgentMetadata {
                agent_base_name,
                agent_title,
                agent_display_name,
                agent_role,
            },
        );
    }

    /// Returns the cached metadata for a thread, defaulting to empty if none has been registered.
    pub(super) fn collab_agent_metadata(&self, thread_id: ThreadId) -> CollabAgentMetadata {
        self.collab_agent_metadata
            .get(&thread_id)
            .cloned()
            .unwrap_or_default()
    }
}
