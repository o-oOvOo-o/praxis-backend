use super::*;

impl AgentOs {
    pub(crate) async fn attach_process_cleaner<T>(self: &Arc<Self>, process_cleaner: Arc<T>)
    where
        T: AgentOsProcessCleaner + 'static,
    {
        let runtime_kind = process_cleaner.runtime_kind().to_string();
        let runtime_owner_id = process_cleaner.runtime_owner_id();
        let exact_key = cleaner_registry_key(runtime_kind.as_str(), runtime_owner_id.as_str());
        let process_cleaner: Arc<dyn AgentOsProcessCleaner> = process_cleaner;
        self.process_cleaners
            .write()
            .await
            .entry(runtime_kind)
            .or_default()
            .push(Arc::clone(&process_cleaner));
        self.process_cleaners_by_owner
            .write()
            .await
            .insert(exact_key, process_cleaner);
        self.start_lease_janitor();
    }

    pub(super) fn start_lease_janitor(self: &Arc<Self>) {
        if self
            .lease_janitor_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let runtime = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                LEASE_JANITOR_INTERVAL_SECONDS,
            ));
            loop {
                interval.tick().await;
                let Some(runtime) = runtime.upgrade() else {
                    break;
                };
                runtime.expire_leases().await;
                runtime.expire_intent_plans().await;
                runtime.expire_runtime_commands().await;
                runtime.expire_tickets().await;
            }
        });
    }
}
