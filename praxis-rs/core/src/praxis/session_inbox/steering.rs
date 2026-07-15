use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::user_input::UserInput;
use std::future;
use std::sync::Arc;

use crate::praxis::Session;
use crate::praxis::SteerInputError;

impl Session {
    /// Inject additional user input into the currently active turn.
    ///
    /// Returns the active turn id when accepted.
    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
    ) -> Result<String, SteerInputError> {
        if input.is_empty() {
            return Err(SteerInputError::EmptyInput);
        }

        let mut active = self.active_turn.lock().await;
        let Some(active_turn) = active.as_mut() else {
            return Err(SteerInputError::NoActiveTurn(input));
        };

        let Some((active_turn_id, active_task)) = active_turn.tasks.first() else {
            return Err(SteerInputError::NoActiveTurn(input));
        };

        if let Some(expected_turn_id) = expected_turn_id
            && expected_turn_id != active_turn_id
        {
            return Err(SteerInputError::ExpectedTurnMismatch {
                expected: expected_turn_id.to_string(),
                actual: active_turn_id.clone(),
            });
        }

        if let Some(turn_kind) = active_task.kind.non_steerable_turn_kind() {
            return Err(SteerInputError::ActiveTurnNotSteerable { turn_kind });
        }

        let active_turn_id = active_turn_id.clone();
        let pending_input_ready = Arc::clone(&active_turn.pending_input_ready);
        let mut turn_state = active_turn.turn_state.lock().await;
        turn_state.push_pending_input(input.into());
        drop(turn_state);
        drop(active);
        pending_input_ready.notify_one();
        Ok(active_turn_id)
    }

    /// Returns the input if there was no task running to inject into.
    pub async fn inject_response_items(
        &self,
        input: Vec<ResponseInputItem>,
    ) -> Result<(), Vec<ResponseInputItem>> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let pending_input_ready = Arc::clone(&at.pending_input_ready);
                let mut ts = at.turn_state.lock().await;
                for item in input {
                    ts.push_pending_input(item);
                }
                drop(ts);
                pending_input_ready.notify_one();
                Ok(())
            }
            None => Err(input),
        }
    }

    pub async fn prepend_pending_input(&self, input: Vec<ResponseInputItem>) -> Result<(), ()> {
        let mut active = self.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let pending_input_ready = Arc::clone(&at.pending_input_ready);
                let mut ts = at.turn_state.lock().await;
                ts.prepend_pending_input(input);
                drop(ts);
                pending_input_ready.notify_one();
                Ok(())
            }
            None => Err(()),
        }
    }

    pub(crate) async fn wait_for_pending_steer(&self) {
        loop {
            let Some((notified, has_pending_input)) = (async {
                let active = self.active_turn.lock().await;
                let active_turn = active.as_ref()?;
                let pending_input_ready = Arc::clone(&active_turn.pending_input_ready);
                let notified = pending_input_ready.notified_owned();
                let has_pending_input = active_turn.turn_state.lock().await.has_pending_input();
                Some((notified, has_pending_input))
            })
            .await
            else {
                future::pending::<()>().await;
                return;
            };

            if has_pending_input {
                return;
            }
            notified.await;
        }
    }
}
