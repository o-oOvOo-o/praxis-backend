use crate::protocol::EventMsg;
use crate::protocol::RolloutItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnLifecycleStatus {
    InProgress,
    Completed,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnLifecycleSnapshot {
    pub id: String,
    pub status: TurnLifecycleStatus,
    pub opened_explicitly: bool,
    pub rollout_start_index: usize,
}

#[derive(Default)]
pub struct TurnLifecycleTracker {
    turns: Vec<TurnLifecycleSnapshot>,
    current_turn: Option<TurnLifecycleSnapshot>,
    current_rollout_index: usize,
    next_rollout_index: usize,
}

impl TurnLifecycleTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_active_turn(&self) -> bool {
        self.current_turn.is_some()
    }

    pub fn active_turn_snapshot(&self) -> Option<&TurnLifecycleSnapshot> {
        self.current_turn.as_ref()
    }

    pub fn active_turn_id_if_explicit(&self) -> Option<String> {
        self.current_turn
            .as_ref()
            .filter(|turn| turn.opened_explicitly)
            .map(|turn| turn.id.clone())
    }

    pub fn active_turn_start_index(&self) -> Option<usize> {
        self.current_turn
            .as_ref()
            .map(|turn| turn.rollout_start_index)
    }

    pub fn handle_rollout_item(&mut self, item: &RolloutItem) {
        self.current_rollout_index = self.next_rollout_index;
        self.next_rollout_index += 1;
        if let RolloutItem::EventMsg(event) = item {
            self.handle_event(event);
        }
    }

    fn handle_event(&mut self, event: &EventMsg) {
        match event {
            EventMsg::TurnStarted(payload) => {
                self.finish_current_turn();
                self.current_turn = Some(TurnLifecycleSnapshot {
                    id: payload.turn_id.clone(),
                    status: TurnLifecycleStatus::InProgress,
                    opened_explicitly: true,
                    rollout_start_index: self.current_rollout_index,
                });
            }
            EventMsg::TurnComplete(payload) => self.handle_turn_complete(&payload.turn_id),
            EventMsg::TurnAborted(payload) => self.handle_turn_aborted(payload.turn_id.as_deref()),
            EventMsg::ThreadRolledBack(payload) => {
                self.finish_current_turn();
                let n = usize::try_from(payload.num_turns).unwrap_or(usize::MAX);
                if n >= self.turns.len() {
                    self.turns.clear();
                } else {
                    self.turns.truncate(self.turns.len().saturating_sub(n));
                }
            }
            EventMsg::Error(payload) if payload.affects_turn_status() => {
                if let Some(turn) = self.current_turn.as_mut() {
                    turn.status = TurnLifecycleStatus::Failed;
                }
            }
            _ => {}
        }
    }

    fn handle_turn_complete(&mut self, turn_id: &str) {
        if let Some(current_turn) = self.current_turn.as_mut().filter(|turn| turn.id == turn_id) {
            mark_completed(&mut current_turn.status);
            self.finish_current_turn();
            return;
        }

        if let Some(turn) = self.turns.iter_mut().find(|turn| turn.id == turn_id) {
            mark_completed(&mut turn.status);
            return;
        }

        if let Some(current_turn) = self.current_turn.as_mut() {
            mark_completed(&mut current_turn.status);
            self.finish_current_turn();
        }
    }

    fn handle_turn_aborted(&mut self, turn_id: Option<&str>) {
        if let Some(turn_id) = turn_id {
            if let Some(turn) = self.current_turn.as_mut().filter(|turn| turn.id == turn_id) {
                turn.status = TurnLifecycleStatus::Interrupted;
                return;
            }

            if let Some(turn) = self.turns.iter_mut().find(|turn| turn.id == turn_id) {
                turn.status = TurnLifecycleStatus::Interrupted;
                return;
            }
        }

        if let Some(turn) = self.current_turn.as_mut() {
            turn.status = TurnLifecycleStatus::Interrupted;
        }
    }

    fn finish_current_turn(&mut self) {
        if let Some(turn) = self.current_turn.take() {
            self.turns.push(turn);
        }
    }
}

fn mark_completed(status: &mut TurnLifecycleStatus) {
    if matches!(
        *status,
        TurnLifecycleStatus::Completed | TurnLifecycleStatus::InProgress
    ) {
        *status = TurnLifecycleStatus::Completed;
    }
}
