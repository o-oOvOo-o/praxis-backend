use super::PraxisMessageProcessor;
use crate::thread_status::ThreadRuntimeState;
use crate::thread_status::ThreadWatchManager;
use crate::thread_status::resolve_thread_status;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadActiveFlag;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_core::ThreadManager;
use praxis_protocol::ThreadId;
use std::collections::HashSet;

pub(super) async fn project_thread_runtime_state_from_watch(
    thread_watch_manager: &ThreadWatchManager,
    thread_manager: &ThreadManager,
    thread: &mut Thread,
    has_live_in_progress_turn: bool,
) {
    let runtime_state = thread_watch_manager
        .loaded_runtime_state_for_thread(&thread.id)
        .await;
    project_thread_runtime_state_values(thread, runtime_state, has_live_in_progress_turn);
    project_resource_wait(thread_manager, thread).await;
}

pub(super) async fn project_thread_runtime_state_with_turn_cleanup_from_watch(
    thread_watch_manager: &ThreadWatchManager,
    thread_manager: &ThreadManager,
    thread: &mut Thread,
    has_live_in_progress_turn: bool,
) {
    project_thread_runtime_state_from_watch(
        thread_watch_manager,
        thread_manager,
        thread,
        has_live_in_progress_turn,
    )
    .await;
    interrupt_stale_turns_for_current_runtime_state(thread, has_live_in_progress_turn);
}

async fn project_resource_wait(thread_manager: &ThreadManager, thread: &mut Thread) {
    let Ok(thread_id) = ThreadId::try_from(thread.id.as_str()) else {
        return;
    };
    if !thread_manager.is_waiting_for_resource(thread_id).await {
        return;
    }

    match &mut thread.status {
        ThreadStatus::Active { active_flags } => {
            if !active_flags.contains(&ThreadActiveFlag::WaitingOnResource) {
                active_flags.push(ThreadActiveFlag::WaitingOnResource);
            }
        }
        status => {
            *status = ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::WaitingOnResource],
            };
        }
    }
}

fn project_thread_runtime_state_values(
    thread: &mut Thread,
    runtime_state: ThreadRuntimeState,
    has_live_in_progress_turn: bool,
) {
    let control_state = runtime_state.control_state;
    thread.status = resolve_thread_status(
        runtime_state.status,
        has_live_in_progress_turn,
        control_state.as_ref(),
    );
    thread.control_state = control_state;
}

fn set_thread_status_and_interrupt_stale_turns(
    thread: &mut Thread,
    loaded_status: ThreadStatus,
    has_live_in_progress_turn: bool,
    control_state: Option<&ThreadControlState>,
) {
    let status = resolve_thread_status(loaded_status, has_live_in_progress_turn, control_state);
    if !matches!(status, ThreadStatus::Active { .. }) {
        for turn in &mut thread.turns {
            if matches!(turn.status, TurnStatus::InProgress) {
                turn.status = TurnStatus::Interrupted;
            }
        }
    }
    thread.status = status;
}

fn interrupt_stale_turns_for_current_runtime_state(
    thread: &mut Thread,
    has_live_in_progress_turn: bool,
) {
    let thread_status = thread.status.clone();
    let control_state = thread.control_state.clone();
    set_thread_status_and_interrupt_stale_turns(
        thread,
        thread_status,
        has_live_in_progress_turn,
        control_state.as_ref(),
    );
}

impl PraxisMessageProcessor {
    pub(super) fn start_agent_os_status_bridge(&self) {
        let mut changes = self.thread_manager.subscribe_coordination_changes();
        let thread_manager = self.thread_manager.clone();
        let thread_watch_manager = self.thread_watch_manager.clone();
        self.background_tasks.spawn(async move {
            let mut previous = HashSet::new();
            loop {
                let current = thread_manager
                    .threads_waiting_for_resources()
                    .await
                    .into_iter()
                    .map(|thread_id| thread_id.to_string())
                    .collect::<HashSet<_>>();

                for thread_id in current.difference(&previous) {
                    thread_watch_manager
                        .note_resource_wait(thread_id, true)
                        .await;
                }
                for thread_id in previous.difference(&current) {
                    thread_watch_manager
                        .note_resource_wait(thread_id, false)
                        .await;
                }
                previous = current;

                if changes.changed().await.is_err() {
                    break;
                }
            }
        });
    }

    pub(super) async fn project_thread_runtime_state(
        &self,
        thread: &mut Thread,
        has_live_in_progress_turn: bool,
    ) {
        project_thread_runtime_state_from_watch(
            &self.thread_watch_manager,
            self.thread_manager.as_ref(),
            thread,
            has_live_in_progress_turn,
        )
        .await;
    }

    pub(super) async fn project_thread_runtime_state_with_turn_cleanup(
        &self,
        thread: &mut Thread,
        has_live_in_progress_turn: bool,
    ) {
        project_thread_runtime_state_with_turn_cleanup_from_watch(
            &self.thread_watch_manager,
            self.thread_manager.as_ref(),
            thread,
            has_live_in_progress_turn,
        )
        .await;
    }

    pub(super) async fn project_thread_runtime_states(&self, threads: Vec<Thread>) -> Vec<Thread> {
        if threads.is_empty() {
            return threads;
        }

        let thread_ids = threads
            .iter()
            .map(|thread| thread.id.clone())
            .collect::<Vec<_>>();
        let mut runtime_states = self
            .thread_watch_manager
            .loaded_runtime_states_for_threads(thread_ids)
            .await;

        let mut projected = Vec::with_capacity(threads.len());
        for mut thread in threads {
            if let Some(runtime_state) = runtime_states.remove(&thread.id) {
                project_thread_runtime_state_values(
                    &mut thread,
                    runtime_state,
                    /*has_live_in_progress_turn*/ false,
                );
            }
            project_resource_wait(self.thread_manager.as_ref(), &mut thread).await;
            projected.push(thread);
        }
        projected
    }
}
