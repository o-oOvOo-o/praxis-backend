use super::*;

impl AgentOs {
    pub(crate) async fn ensure_inter_thread_message_allowed(
        &self,
        from_thread_id: ThreadId,
        to_thread_id: ThreadId,
        require_active_dispatcher: bool,
    ) -> PraxisResult<()> {
        let mut state = self.state.write().await;
        let sender = state.threads.get(&from_thread_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS sender thread `{from_thread_id}`"
            ))
        })?;
        let receiver = state.threads.get(&to_thread_id).cloned().ok_or_else(|| {
            PraxisErr::UnsupportedOperation(format!(
                "unknown AgentOS receiver thread `{to_thread_id}`"
            ))
        })?;
        if sender.rank != COORDINATOR_RANK {
            return Err(PraxisErr::UnsupportedOperation(
                "worker-to-worker natural-language messaging is disabled by AgentOS; submit artifacts, status, or structured requests instead".to_string(),
            ));
        }
        if receiver.coordination_scope != sender.coordination_scope {
            return Err(PraxisErr::UnsupportedOperation(
                "inter-thread messaging cannot cross coordination scopes".to_string(),
            ));
        }
        if require_active_dispatcher {
            Self::claim_or_renew_active_coordinator_locked(
                &mut state,
                &sender,
                Utc::now(),
                Some("dispatch tasks"),
            )?;
        }
        Ok(())
    }
}
