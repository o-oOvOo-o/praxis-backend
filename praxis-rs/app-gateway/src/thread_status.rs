#[cfg(test)]
use crate::outgoing_message::OutgoingEnvelope;
#[cfg(test)]
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use praxis_app_gateway_protocol::ServerNotification;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadActiveFlag;
use praxis_app_gateway_protocol::ThreadControlChangedNotification;
use praxis_app_gateway_protocol::ThreadControlState;
use praxis_app_gateway_protocol::ThreadController;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::ThreadStatusChangedNotification;
use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;

#[derive(Clone)]
pub(crate) struct ThreadWatchManager {
    state: Arc<Mutex<ThreadWatchState>>,
    outgoing: Option<Arc<OutgoingMessageSender>>,
    running_turn_count_tx: watch::Sender<usize>,
}

pub(crate) struct ThreadWatchActiveGuard {
    manager: ThreadWatchManager,
    thread_id: String,
    guard_type: ThreadWatchActiveGuardType,
    handle: tokio::runtime::Handle,
}

#[derive(Debug)]
pub(crate) struct ThreadRuntimeState {
    pub(crate) status: ThreadStatus,
    pub(crate) control_state: Option<ThreadControlState>,
}

impl ThreadWatchActiveGuard {
    fn new(
        manager: ThreadWatchManager,
        thread_id: String,
        guard_type: ThreadWatchActiveGuardType,
    ) -> Self {
        Self {
            manager,
            thread_id,
            guard_type,
            handle: tokio::runtime::Handle::current(),
        }
    }
}

impl Drop for ThreadWatchActiveGuard {
    fn drop(&mut self) {
        let manager = self.manager.clone();
        let thread_id = self.thread_id.clone();
        let guard_type = self.guard_type;
        self.handle.spawn(async move {
            manager
                .note_active_guard_released(thread_id, guard_type)
                .await;
        });
    }
}

#[derive(Clone, Copy)]
enum ThreadWatchActiveGuardType {
    Permission,
    UserInput,
}

impl Default for ThreadWatchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadWatchManager {
    pub(crate) fn new() -> Self {
        let (running_turn_count_tx, _running_turn_count_rx) = watch::channel(0);
        Self {
            state: Arc::new(Mutex::new(ThreadWatchState::default())),
            outgoing: None,
            running_turn_count_tx,
        }
    }

    pub(crate) fn new_with_outgoing(outgoing: Arc<OutgoingMessageSender>) -> Self {
        let (running_turn_count_tx, _running_turn_count_rx) = watch::channel(0);
        Self {
            state: Arc::new(Mutex::new(ThreadWatchState::default())),
            outgoing: Some(outgoing),
            running_turn_count_tx,
        }
    }

    pub(crate) async fn upsert_thread(&self, thread: Thread) {
        self.mutate_and_publish(move |state| {
            state.upsert_thread(thread.id, /*emit_notification*/ true)
        })
        .await;
    }

    pub(crate) async fn upsert_thread_silently(&self, thread: Thread) {
        self.mutate_and_publish(move |state| {
            state.upsert_thread(thread.id, /*emit_notification*/ false)
        })
        .await;
    }

    pub(crate) async fn remove_thread(&self, thread_id: &str) {
        self.release_thread_control(thread_id).await;
        let thread_id = thread_id.to_string();
        self.mutate_and_publish(move |state| state.remove_thread(&thread_id))
            .await;
    }

    #[cfg(test)]
    async fn loaded_status_for_thread(&self, thread_id: &str) -> ThreadStatus {
        self.state.lock().await.loaded_status_for_thread(thread_id)
    }

    #[cfg(test)]
    async fn loaded_control_state_for_thread(&self, thread_id: &str) -> Option<ThreadControlState> {
        self.state
            .lock()
            .await
            .loaded_control_state_for_thread(thread_id)
    }

    pub(crate) async fn loaded_runtime_state_for_thread(
        &self,
        thread_id: &str,
    ) -> ThreadRuntimeState {
        self.state
            .lock()
            .await
            .loaded_runtime_state_for_thread(thread_id)
    }

    pub(crate) async fn loaded_runtime_states_for_threads(
        &self,
        thread_ids: Vec<String>,
    ) -> HashMap<String, ThreadRuntimeState> {
        let state = self.state.lock().await;
        thread_ids
            .into_iter()
            .map(|thread_id| {
                let runtime_state = state.loaded_runtime_state_for_thread(&thread_id);
                (thread_id, runtime_state)
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) async fn running_turn_count(&self) -> usize {
        self.state
            .lock()
            .await
            .runtime_by_thread_id
            .values()
            .filter(|runtime| runtime.running)
            .count()
    }

    pub(crate) fn subscribe_running_turn_count(&self) -> watch::Receiver<usize> {
        self.running_turn_count_tx.subscribe()
    }

    pub(crate) async fn note_turn_started(&self, thread_id: &str) {
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.is_loaded = true;
            runtime.running = true;
            runtime.has_system_error = false;
        })
        .await;
    }

    pub(crate) async fn note_turn_completed(&self, thread_id: &str, _failed: bool) {
        self.clear_active_state(thread_id).await;
    }

    pub(crate) async fn note_turn_interrupted(&self, thread_id: &str) {
        self.clear_active_state(thread_id).await;
    }

    pub(crate) async fn note_thread_shutdown(&self, thread_id: &str) {
        self.release_thread_control(thread_id).await;
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.running = false;
            runtime.pending_permission_requests = 0;
            runtime.pending_user_input_requests = 0;
            runtime.is_loaded = false;
        })
        .await;
    }

    pub(crate) async fn note_system_error(&self, thread_id: &str) {
        self.release_thread_control(thread_id).await;
        self.update_runtime_for_thread(thread_id, |runtime| {
            runtime.running = false;
            runtime.pending_permission_requests = 0;
            runtime.pending_user_input_requests = 0;
            runtime.has_system_error = true;
        })
        .await;
    }

    async fn clear_active_state(&self, thread_id: &str) {
        let thread_id = thread_id.to_string();
        let (status_notification, running_turn_count) = {
            let mut state = self.state.lock().await;
            let previous_status = state.status_for(&thread_id);
            let runtime = state
                .runtime_by_thread_id
                .entry(thread_id.clone())
                .or_default();
            runtime.is_loaded = true;
            runtime.running = false;
            runtime.pending_permission_requests = 0;
            runtime.pending_user_input_requests = 0;
            let status_notification =
                state.status_changed_notification(thread_id.clone(), previous_status);
            let running_turn_count = state.running_turn_count();
            (status_notification, running_turn_count)
        };
        self.publish_runtime_notifications(status_notification, None, running_turn_count)
            .await;
    }

    pub(crate) async fn acquire_thread_control(
        &self,
        thread_id: &str,
        controller: ThreadController,
        reason: Option<String>,
    ) -> ThreadControlState {
        let thread_id = thread_id.to_string();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let (control_state, status_notification, control_notification, running_turn_count) =
            {
                let mut state = self.state.lock().await;
                let previous_status = state.status_for(&thread_id);
                let previous_control = state.loaded_control_state_for_thread(&thread_id);
                let acquired_at = previous_control
                    .as_ref()
                    .filter(|control| control.controller == controller)
                    .map(|control| control.acquired_at)
                    .unwrap_or(now);
                let control_state = ThreadControlState {
                    controller,
                    reason,
                    read_only: true,
                    acquired_at,
                    updated_at: now,
                };
                let runtime = state
                    .runtime_by_thread_id
                    .entry(thread_id.clone())
                    .or_default();
                runtime.is_loaded = true;
                runtime.control_state = Some(control_state.clone());

                let status_notification =
                    state.status_changed_notification(thread_id.clone(), previous_status);
                let control_notification = (previous_control.as_ref() != Some(&control_state))
                    .then(|| ThreadControlChangedNotification {
                        thread_id: thread_id.clone(),
                        control_state: Some(control_state.clone()),
                    });
                let running_turn_count = state.running_turn_count();
                (
                    control_state,
                    status_notification,
                    control_notification,
                    running_turn_count,
                )
            };
        self.publish_runtime_notifications(
            status_notification,
            control_notification,
            running_turn_count,
        )
        .await;
        control_state
    }

    pub(crate) async fn release_thread_control(
        &self,
        thread_id: &str,
    ) -> Option<ThreadControlState> {
        let thread_id = thread_id.to_string();
        let (previous_control, status_notification, control_notification, running_turn_count) = {
            let mut state = self.state.lock().await;
            let previous_status = state.status_for(&thread_id);
            let previous_control = state
                .runtime_by_thread_id
                .get_mut(&thread_id)
                .and_then(|runtime| runtime.control_state.take());
            let status_notification =
                state.status_changed_notification(thread_id.clone(), previous_status);
            let control_notification =
                previous_control
                    .as_ref()
                    .map(|_| ThreadControlChangedNotification {
                        thread_id: thread_id.clone(),
                        control_state: None,
                    });
            let running_turn_count = state.running_turn_count();
            (
                previous_control,
                status_notification,
                control_notification,
                running_turn_count,
            )
        };
        self.publish_runtime_notifications(
            status_notification,
            control_notification,
            running_turn_count,
        )
        .await;
        previous_control
    }

    pub(crate) async fn note_permission_requested(
        &self,
        thread_id: &str,
    ) -> ThreadWatchActiveGuard {
        self.note_pending_request(thread_id, ThreadWatchActiveGuardType::Permission)
            .await
    }

    pub(crate) async fn note_user_input_requested(
        &self,
        thread_id: &str,
    ) -> ThreadWatchActiveGuard {
        self.note_pending_request(thread_id, ThreadWatchActiveGuardType::UserInput)
            .await
    }

    async fn note_pending_request(
        &self,
        thread_id: &str,
        guard_type: ThreadWatchActiveGuardType,
    ) -> ThreadWatchActiveGuard {
        self.update_runtime_for_thread(thread_id, move |runtime| {
            runtime.is_loaded = true;
            let counter = Self::pending_counter(runtime, guard_type);
            *counter = counter.saturating_add(1);
        })
        .await;
        ThreadWatchActiveGuard::new(self.clone(), thread_id.to_string(), guard_type)
    }

    async fn mutate_and_publish<F>(&self, mutate: F)
    where
        F: FnOnce(&mut ThreadWatchState) -> Option<ThreadStatusChangedNotification>,
    {
        let (notification, running_turn_count) = {
            let mut state = self.state.lock().await;
            let notification = mutate(&mut state);
            let running_turn_count = state.running_turn_count();
            (notification, running_turn_count)
        };
        self.publish_runtime_notifications(notification, None, running_turn_count)
            .await;
    }

    async fn publish_runtime_notifications(
        &self,
        status_notification: Option<ThreadStatusChangedNotification>,
        control_notification: Option<ThreadControlChangedNotification>,
        running_turn_count: usize,
    ) {
        let _ = self.running_turn_count_tx.send(running_turn_count);

        if let Some(outgoing) = &self.outgoing {
            if let Some(notification) = status_notification {
                outgoing
                    .send_server_notification(ServerNotification::ThreadStatusChanged(notification))
                    .await;
            }
            if let Some(notification) = control_notification {
                outgoing
                    .send_server_notification(ServerNotification::ThreadControlChanged(
                        notification,
                    ))
                    .await;
            }
        }
    }

    async fn note_active_guard_released(
        &self,
        thread_id: String,
        guard_type: ThreadWatchActiveGuardType,
    ) {
        self.update_runtime_for_thread(&thread_id, move |runtime| {
            let counter = Self::pending_counter(runtime, guard_type);
            *counter = counter.saturating_sub(1);
        })
        .await;
    }

    async fn update_runtime_for_thread<F>(&self, thread_id: &str, update: F)
    where
        F: FnOnce(&mut RuntimeFacts),
    {
        let thread_id = thread_id.to_string();
        self.mutate_and_publish(move |state| state.update_runtime(&thread_id, update))
            .await;
    }

    fn pending_counter(
        runtime: &mut RuntimeFacts,
        guard_type: ThreadWatchActiveGuardType,
    ) -> &mut u32 {
        match guard_type {
            ThreadWatchActiveGuardType::Permission => &mut runtime.pending_permission_requests,
            ThreadWatchActiveGuardType::UserInput => &mut runtime.pending_user_input_requests,
        }
    }
}

pub(crate) fn resolve_thread_status(
    status: ThreadStatus,
    has_in_progress_turn: bool,
    control_state: Option<&ThreadControlState>,
) -> ThreadStatus {
    let is_controlled = control_state.is_some();
    match status {
        ThreadStatus::Active { mut active_flags } => {
            push_unique_active_flag(
                &mut active_flags,
                has_in_progress_turn,
                ThreadActiveFlag::Running,
            );
            push_unique_active_flag(
                &mut active_flags,
                is_controlled,
                ThreadActiveFlag::Controlled,
            );
            ThreadStatus::Active { active_flags }
        }
        ThreadStatus::Idle | ThreadStatus::NotLoaded if has_in_progress_turn || is_controlled => {
            let mut active_flags = Vec::new();
            push_unique_active_flag(
                &mut active_flags,
                has_in_progress_turn,
                ThreadActiveFlag::Running,
            );
            push_unique_active_flag(
                &mut active_flags,
                is_controlled,
                ThreadActiveFlag::Controlled,
            );
            ThreadStatus::Active { active_flags }
        }
        status => status,
    }
}

fn push_unique_active_flag(
    active_flags: &mut Vec<ThreadActiveFlag>,
    should_insert: bool,
    flag: ThreadActiveFlag,
) {
    if should_insert && !active_flags.contains(&flag) {
        active_flags.push(flag);
    }
}

#[derive(Default)]
struct ThreadWatchState {
    runtime_by_thread_id: HashMap<String, RuntimeFacts>,
}

impl ThreadWatchState {
    fn upsert_thread(
        &mut self,
        thread_id: String,
        emit_notification: bool,
    ) -> Option<ThreadStatusChangedNotification> {
        let previous_status = self.status_for(&thread_id);
        let runtime = self
            .runtime_by_thread_id
            .entry(thread_id.clone())
            .or_default();
        runtime.is_loaded = true;
        if emit_notification {
            self.status_changed_notification(thread_id, previous_status)
        } else {
            None
        }
    }

    fn remove_thread(&mut self, thread_id: &str) -> Option<ThreadStatusChangedNotification> {
        let previous_status = self.status_for(thread_id);
        self.runtime_by_thread_id.remove(thread_id);
        if previous_status.is_some() && previous_status != Some(ThreadStatus::NotLoaded) {
            Some(ThreadStatusChangedNotification {
                thread_id: thread_id.to_string(),
                status: ThreadStatus::NotLoaded,
            })
        } else {
            None
        }
    }

    fn update_runtime<F>(
        &mut self,
        thread_id: &str,
        mutate: F,
    ) -> Option<ThreadStatusChangedNotification>
    where
        F: FnOnce(&mut RuntimeFacts),
    {
        let previous_status = self.status_for(thread_id);
        let runtime = self
            .runtime_by_thread_id
            .entry(thread_id.to_string())
            .or_default();
        runtime.is_loaded = true;
        mutate(runtime);
        self.status_changed_notification(thread_id.to_string(), previous_status)
    }

    fn status_for(&self, thread_id: &str) -> Option<ThreadStatus> {
        self.runtime_by_thread_id
            .get(thread_id)
            .map(loaded_thread_status)
    }

    fn loaded_status_for_thread(&self, thread_id: &str) -> ThreadStatus {
        self.status_for(thread_id)
            .unwrap_or(ThreadStatus::NotLoaded)
    }

    fn loaded_control_state_for_thread(&self, thread_id: &str) -> Option<ThreadControlState> {
        self.runtime_by_thread_id
            .get(thread_id)
            .and_then(|runtime| runtime.control_state.clone())
    }

    fn loaded_runtime_state_for_thread(&self, thread_id: &str) -> ThreadRuntimeState {
        ThreadRuntimeState {
            status: self.loaded_status_for_thread(thread_id),
            control_state: self.loaded_control_state_for_thread(thread_id),
        }
    }

    fn running_turn_count(&self) -> usize {
        self.runtime_by_thread_id
            .values()
            .filter(|runtime| runtime.running)
            .count()
    }

    fn status_changed_notification(
        &self,
        thread_id: String,
        previous_status: Option<ThreadStatus>,
    ) -> Option<ThreadStatusChangedNotification> {
        let status = self.status_for(&thread_id)?;

        if previous_status.as_ref() == Some(&status) {
            return None;
        }

        Some(ThreadStatusChangedNotification { thread_id, status })
    }
}

#[derive(Clone, Default)]
struct RuntimeFacts {
    is_loaded: bool,
    running: bool,
    pending_permission_requests: u32,
    pending_user_input_requests: u32,
    has_system_error: bool,
    control_state: Option<ThreadControlState>,
}

fn loaded_thread_status(runtime: &RuntimeFacts) -> ThreadStatus {
    if !runtime.is_loaded {
        return ThreadStatus::NotLoaded;
    }

    let mut active_flags = Vec::new();
    if runtime.running {
        active_flags.push(ThreadActiveFlag::Running);
    }
    if runtime.pending_permission_requests > 0 {
        active_flags.push(ThreadActiveFlag::WaitingOnApproval);
    }
    if runtime.pending_user_input_requests > 0 {
        active_flags.push(ThreadActiveFlag::WaitingOnUserInput);
    }
    if runtime.control_state.is_some() {
        active_flags.push(ThreadActiveFlag::Controlled);
    }

    if !active_flags.is_empty() {
        return ThreadStatus::Active { active_flags };
    }

    if runtime.has_system_error {
        return ThreadStatus::SystemError;
    }

    ThreadStatus::Idle
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tokio::time::Duration;
    use tokio::time::timeout;

    const INTERACTIVE_THREAD_ID: &str = "00000000-0000-0000-0000-000000000001";
    const NON_INTERACTIVE_THREAD_ID: &str = "00000000-0000-0000-0000-000000000002";

    #[tokio::test]
    async fn loaded_status_defaults_to_not_loaded_for_untracked_threads() {
        let manager = ThreadWatchManager::new();

        assert_eq!(
            manager
                .loaded_status_for_thread("00000000-0000-0000-0000-000000000003")
                .await,
            ThreadStatus::NotLoaded,
        );
    }

    #[tokio::test]
    async fn tracks_non_interactive_thread_status() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                NON_INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::AppGateway,
            ))
            .await;

        manager.note_turn_started(NON_INTERACTIVE_THREAD_ID).await;

        assert_eq!(
            manager
                .loaded_status_for_thread(NON_INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            },
        );
    }

    #[tokio::test]
    async fn status_updates_track_single_thread() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            },
        );

        let permission_guard = manager
            .note_permission_requested(INTERACTIVE_THREAD_ID)
            .await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![
                    ThreadActiveFlag::Running,
                    ThreadActiveFlag::WaitingOnApproval,
                ],
            },
        );

        let user_input_guard = manager
            .note_user_input_requested(INTERACTIVE_THREAD_ID)
            .await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![
                    ThreadActiveFlag::Running,
                    ThreadActiveFlag::WaitingOnApproval,
                    ThreadActiveFlag::WaitingOnUserInput,
                ],
            },
        );

        drop(permission_guard);
        wait_for_status(
            &manager,
            INTERACTIVE_THREAD_ID,
            ThreadStatus::Active {
                active_flags: vec![
                    ThreadActiveFlag::Running,
                    ThreadActiveFlag::WaitingOnUserInput,
                ],
            },
        )
        .await;

        drop(user_input_guard);
        wait_for_status(
            &manager,
            INTERACTIVE_THREAD_ID,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            },
        )
        .await;

        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Idle,
        );
    }

    #[test]
    fn resolves_in_progress_turn_to_active_status() {
        let status =
            resolve_thread_status(ThreadStatus::Idle, /*has_in_progress_turn*/ true, None);
        assert_eq!(
            status,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            }
        );

        let status = resolve_thread_status(
            ThreadStatus::NotLoaded,
            /*has_in_progress_turn*/ true,
            None,
        );
        assert_eq!(
            status,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            }
        );
    }

    #[test]
    fn resolves_controlled_in_progress_turn_to_running_controlled_status() {
        let control_state = ThreadControlState {
            controller: ThreadController {
                kind: praxis_app_gateway_protocol::ThreadControllerKind::External,
                id: "test-controller".to_string(),
                label: Some("test controller".to_string()),
                rank: Some(0),
            },
            reason: Some("test lock".to_string()),
            read_only: true,
            acquired_at: 1,
            updated_at: 1,
        };

        let status = resolve_thread_status(
            ThreadStatus::Idle,
            /*has_in_progress_turn*/ true,
            Some(&control_state),
        );
        assert_eq!(
            status,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running, ThreadActiveFlag::Controlled],
            }
        );
    }

    #[test]
    fn keeps_status_when_no_in_progress_turn() {
        assert_eq!(
            resolve_thread_status(
                ThreadStatus::Idle,
                /*has_in_progress_turn*/ false,
                None
            ),
            ThreadStatus::Idle
        );
        assert_eq!(
            resolve_thread_status(
                ThreadStatus::SystemError,
                /*has_in_progress_turn*/ false,
                None
            ),
            ThreadStatus::SystemError
        );
    }

    #[tokio::test]
    async fn system_error_sets_idle_flag_until_next_turn() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        manager.note_system_error(INTERACTIVE_THREAD_ID).await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::SystemError,
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            },
        );
    }

    #[tokio::test]
    async fn shutdown_marks_thread_not_loaded() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        manager.note_thread_shutdown(INTERACTIVE_THREAD_ID).await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::NotLoaded,
        );
    }

    #[tokio::test]
    async fn loaded_runtime_states_default_to_not_loaded_for_untracked_threads() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;
        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;

        let runtime_states = manager
            .loaded_runtime_states_for_threads(vec![
                INTERACTIVE_THREAD_ID.to_string(),
                NON_INTERACTIVE_THREAD_ID.to_string(),
            ])
            .await;

        assert_eq!(
            runtime_states
                .get(INTERACTIVE_THREAD_ID)
                .map(|state| &state.status),
            Some(&ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Running],
            }),
        );
        assert_eq!(
            runtime_states
                .get(NON_INTERACTIVE_THREAD_ID)
                .map(|state| &state.status),
            Some(&ThreadStatus::NotLoaded),
        );
    }

    #[tokio::test]
    async fn has_running_turns_tracks_runtime_running_flag_only() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;

        assert_eq!(manager.running_turn_count().await, 0);

        let _permission_guard = manager
            .note_permission_requested(INTERACTIVE_THREAD_ID)
            .await;
        assert_eq!(manager.running_turn_count().await, 0);

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(manager.running_turn_count().await, 1);

        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;
        assert_eq!(manager.running_turn_count().await, 0);
    }

    #[tokio::test]
    async fn control_lock_survives_turn_completion_until_explicit_release() {
        let manager = ThreadWatchManager::new();
        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;
        let controller = ThreadController {
            kind: praxis_app_gateway_protocol::ThreadControllerKind::External,
            id: "test-controller".to_string(),
            label: Some("test controller".to_string()),
            rank: Some(0),
        };

        let control_state = manager
            .acquire_thread_control(
                INTERACTIVE_THREAD_ID,
                controller,
                Some("test lock".to_string()),
            )
            .await;
        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        manager
            .note_turn_completed(INTERACTIVE_THREAD_ID, false)
            .await;

        assert_eq!(
            manager
                .loaded_control_state_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            Some(control_state.clone()),
        );
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Active {
                active_flags: vec![ThreadActiveFlag::Controlled],
            },
        );

        assert_eq!(
            manager.release_thread_control(INTERACTIVE_THREAD_ID).await,
            Some(control_state),
        );
        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Idle,
        );
    }

    #[tokio::test]
    async fn status_change_emits_notification() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);
        let manager = ThreadWatchManager::new_with_outgoing(Arc::new(OutgoingMessageSender::new(
            outgoing_tx,
        )));

        manager
            .upsert_thread(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::Idle,
            },
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::Running],
                },
            },
        );

        manager.remove_thread(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::NotLoaded,
            },
        );
    }

    #[tokio::test]
    async fn silent_upsert_skips_initial_notification() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel(8);
        let manager = ThreadWatchManager::new_with_outgoing(Arc::new(OutgoingMessageSender::new(
            outgoing_tx,
        )));

        manager
            .upsert_thread_silently(test_thread(
                INTERACTIVE_THREAD_ID,
                praxis_app_gateway_protocol::SessionSource::Cli,
            ))
            .await;

        assert_eq!(
            manager
                .loaded_status_for_thread(INTERACTIVE_THREAD_ID)
                .await,
            ThreadStatus::Idle,
        );
        assert!(
            timeout(Duration::from_millis(100), outgoing_rx.recv())
                .await
                .is_err(),
            "silent upsert should not emit thread/status/changed"
        );

        manager.note_turn_started(INTERACTIVE_THREAD_ID).await;
        assert_eq!(
            recv_status_changed_notification(&mut outgoing_rx).await,
            ThreadStatusChangedNotification {
                thread_id: INTERACTIVE_THREAD_ID.to_string(),
                status: ThreadStatus::Active {
                    active_flags: vec![ThreadActiveFlag::Running],
                },
            },
        );
    }

    async fn wait_for_status(
        manager: &ThreadWatchManager,
        thread_id: &str,
        expected_status: ThreadStatus,
    ) {
        timeout(Duration::from_secs(1), async {
            loop {
                let status = manager.loaded_status_for_thread(thread_id).await;
                if status == expected_status {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("timed out waiting for status");
    }

    async fn recv_status_changed_notification(
        outgoing_rx: &mut mpsc::Receiver<OutgoingEnvelope>,
    ) -> ThreadStatusChangedNotification {
        let envelope = timeout(Duration::from_secs(1), outgoing_rx.recv())
            .await
            .expect("timed out waiting for outgoing notification")
            .expect("outgoing channel closed unexpectedly");
        let OutgoingEnvelope::Broadcast { message } = envelope else {
            panic!("expected broadcast notification");
        };
        let OutgoingMessage::AppGatewayNotification(ServerNotification::ThreadStatusChanged(
            notification,
        )) = message
        else {
            panic!("expected thread/status/changed notification");
        };
        notification
    }

    fn test_thread(thread_id: &str, source: praxis_app_gateway_protocol::SessionSource) -> Thread {
        Thread {
            id: thread_id.to_string(),
            preview: String::new(),
            summary: None,
            ephemeral: false,
            model_provider: "mock-provider".to_string(),
            model: None,
            created_at: 0,
            updated_at: 0,
            status: ThreadStatus::NotLoaded,
            path: None,
            cwd: PathBuf::from("/tmp"),
            cli_version: "test".to_string(),
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            source,
            git_info: None,
            name: None,
            total_cost_usd: None,
            last_cost_usd: None,
            token_usage: None,
            control_state: None,
            selfwork_plan_path: None,
            turns: Vec::new(),
        }
    }
}
