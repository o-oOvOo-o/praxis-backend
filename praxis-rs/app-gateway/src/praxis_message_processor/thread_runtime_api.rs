use super::PraxisMessageProcessor;
use crate::thread_status::ThreadRuntimeState;
use crate::thread_status::ThreadWatchManager;
use crate::thread_status::resolve_thread_status;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::TurnStatus;

pub(super) async fn project_thread_runtime_state_from_watch(
    thread_watch_manager: &ThreadWatchManager,
    thread: &mut Thread,
    has_live_in_progress_turn: bool,
) {
    let runtime_state = thread_watch_manager
        .loaded_runtime_state_for_thread(&thread.id)
        .await;
    project_thread_runtime_state_values(thread, runtime_state, has_live_in_progress_turn);
}

pub(super) async fn project_thread_runtime_state_with_turn_cleanup_from_watch(
    thread_watch_manager: &ThreadWatchManager,
    thread: &mut Thread,
    has_live_in_progress_turn: bool,
) {
    project_thread_runtime_state_from_watch(
        thread_watch_manager,
        thread,
        has_live_in_progress_turn,
    )
    .await;
    interrupt_stale_turns_for_current_runtime_state(thread, has_live_in_progress_turn);
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
    pub(super) async fn project_thread_runtime_state(
        &self,
        thread: &mut Thread,
        has_live_in_progress_turn: bool,
    ) {
        project_thread_runtime_state_from_watch(
            &self.thread_watch_manager,
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

        threads
            .into_iter()
            .map(|mut thread| {
                if let Some(runtime_state) = runtime_states.remove(&thread.id) {
                    project_thread_runtime_state_values(
                        &mut thread,
                        runtime_state,
                        /*has_live_in_progress_turn*/ false,
                    );
                }
                thread
            })
            .collect()
    }
}
