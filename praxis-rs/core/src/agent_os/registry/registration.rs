use super::*;

impl AgentOs {
    pub(crate) async fn register_thread(
        &self,
        registration: ThreadRegistration,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let entry = ThreadRegistryEntry {
            thread_id: registration.thread_id,
            coordination_scope: registration.coordination_scope,
            rank: registration.rank,
            profile_id: registration.profile_id,
            cwd: registration.cwd,
            repo_id: registration.repo_id,
            branch: registration.branch,
            worktree: registration.worktree,
            current_task_id: None,
            current_command_id: None,
            state: ThreadRuntimeState::Idle,
            heartbeat_at: now,
            priority: registration.priority,
            created_at: now,
        };

        {
            let mut state = self.state.write().await;
            state.ensure_builtin_profiles();
            if entry.rank == COORDINATOR_RANK {
                Self::register_rank_zero_coordinator_locked(&mut state, &entry, now)?;
            }
            state.threads.insert(entry.thread_id, entry.clone());
        }

        self.persist_thread_snapshot(&entry).await;
        self.record_event(
            "thread_registered",
            Some(entry.thread_id),
            None,
            None,
            json!({
                "coordination_scope": entry.coordination_scope,
                "rank": entry.rank,
                "profile_id": entry.profile_id,
                "cwd": entry.cwd,
            }),
        )
        .await;
        Ok(())
    }

    fn register_rank_zero_coordinator_locked(
        state: &mut AgentOsState,
        entry: &ThreadRegistryEntry,
        now: chrono::DateTime<Utc>,
    ) -> PraxisResult<()> {
        let coordinator_count = state
            .threads
            .values()
            .filter(|thread| thread.rank == COORDINATOR_RANK)
            .filter(|thread| thread.coordination_scope == entry.coordination_scope)
            .filter(|thread| thread.thread_id != entry.thread_id)
            .filter(|thread| {
                !matches!(
                    thread.state,
                    ThreadRuntimeState::Stopped
                        | ThreadRuntimeState::Failed
                        | ThreadRuntimeState::Completed
                )
            })
            .count();
        if coordinator_count >= MAX_COORDINATORS {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "rank-0 coordinator limit reached for scope `{}`",
                entry.coordination_scope
            )));
        }
        let active_state = state
            .active_coordinators
            .get(entry.coordination_scope.as_str())
            .map(|active| (active.owner_thread_id, active.expires_at));
        match active_state {
            Some((owner, expires_at)) if owner == entry.thread_id && expires_at > now => {
                if let Some(active) = state
                    .active_coordinators
                    .get_mut(entry.coordination_scope.as_str())
                {
                    active.expires_at = now + AgentOsPolicy::get().lease_ttl();
                }
            }
            Some((_owner, expires_at)) if expires_at > now => {}
            _ => {
                state.coordinator_epoch = state.coordinator_epoch.saturating_add(1);
                state.fencing_counter = state.fencing_counter.saturating_add(1);
                let epoch = state.coordinator_epoch;
                let fencing_token = state.fencing_counter;
                state.active_coordinators.insert(
                    entry.coordination_scope.clone(),
                    ActiveCoordinatorLease {
                        coordination_scope: entry.coordination_scope.clone(),
                        owner_thread_id: entry.thread_id,
                        epoch,
                        fencing_token,
                        expires_at: now + AgentOsPolicy::get().lease_ttl(),
                    },
                );
            }
        }
        Ok(())
    }
}
