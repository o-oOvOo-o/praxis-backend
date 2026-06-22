use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn cleanup_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> bool {
        let (runtime_kind, process_owner_id) = {
            let state = self.state.read().await;
            let process_key = process_registry_key(process_id, runtime_owner_id);
            state
                .processes
                .get(process_key.as_str())
                .map(|process| {
                    (
                        Some(process.runtime_kind.clone()),
                        process.runtime_owner_id.clone(),
                    )
                })
                .unwrap_or((None, runtime_owner_id.map(str::to_string)))
        };

        if let (Some(runtime_kind), Some(process_owner_id)) =
            (runtime_kind.as_deref(), process_owner_id.as_deref())
        {
            let exact_key = cleaner_registry_key(runtime_kind, process_owner_id);
            let cleaner = self
                .process_cleaners_by_owner
                .read()
                .await
                .get(exact_key.as_str())
                .cloned();
            if let Some(cleaner) = cleaner
                && cleaner.cleanup_agent_os_process(process_id).await
            {
                return true;
            }
        }

        let cleaners = {
            let cleaners_by_kind = self.process_cleaners.read().await;
            let mut selected = Vec::new();

            if process_owner_id.is_none() {
                if let Some(runtime_kind) = runtime_kind.as_deref()
                    && let Some(cleaners) = cleaners_by_kind.get(runtime_kind)
                {
                    selected.extend(cleaners.iter().cloned());
                }
            }
            if let Some(cleaners) = cleaners_by_kind.get(process_runtime_kind::GENERIC) {
                selected.extend(cleaners.iter().cloned());
            }
            if selected.is_empty() && process_owner_id.is_none() {
                selected.extend(cleaners_by_kind.values().flatten().cloned());
            }
            selected
        };
        for cleaner in cleaners {
            if cleaner.cleanup_agent_os_process(process_id).await {
                return true;
            }
        }
        false
    }
}
