use super::*;

impl ThreadHistoryBuilder {
    pub(super) fn handle_context_compacted(&mut self, _payload: &ContextCompactedEvent) {
        let id = self.next_item_id();
        self.ensure_turn()
            .items
            .push(ThreadItem::ContextCompaction { id });
    }

    pub(super) fn handle_entered_review_mode(
        &mut self,
        payload: &praxis_protocol::protocol::ReviewRequest,
    ) {
        let review = payload
            .user_facing_hint
            .clone()
            .unwrap_or_else(|| "Review requested.".to_string());
        let id = self.next_item_id();
        self.ensure_turn()
            .items
            .push(ThreadItem::EnteredReviewMode { id, review });
    }

    pub(super) fn handle_exited_review_mode(
        &mut self,
        payload: &praxis_protocol::protocol::ExitedReviewModeEvent,
    ) {
        let review = payload
            .review_output
            .as_ref()
            .map(render_review_output_text)
            .unwrap_or_else(|| REVIEW_FALLBACK_MESSAGE.to_string());
        let id = self.next_item_id();
        self.ensure_turn()
            .items
            .push(ThreadItem::ExitedReviewMode { id, review });
    }

    pub(super) fn handle_error(&mut self, payload: &ErrorEvent) {
        if !payload.affects_turn_status() {
            return;
        }
        let Some(turn) = self.current_turn.as_mut() else {
            return;
        };
        turn.status = TurnStatus::Failed;
        turn.error = Some(ApiTurnError {
            message: payload.message.clone(),
            praxis_error_info: payload.praxis_error_info.clone().map(Into::into),
            additional_details: None,
        });
    }

    pub(super) fn handle_turn_aborted(&mut self, payload: &TurnAbortedEvent) {
        if let Some(turn_id) = payload.turn_id.as_deref() {
            // Prefer an exact ID match so we interrupt the turn explicitly targeted by the event.
            if let Some(turn) = self.current_turn.as_mut().filter(|turn| turn.id == turn_id) {
                turn.status = TurnStatus::Interrupted;
                return;
            }

            if let Some(turn) = self.turns.iter_mut().find(|turn| turn.id == turn_id) {
                turn.status = TurnStatus::Interrupted;
                return;
            }
        }

        // If the event has no ID (or refers to an unknown turn), fall back to the active turn.
        if let Some(turn) = self.current_turn.as_mut() {
            turn.status = TurnStatus::Interrupted;
        }
    }

    pub(super) fn handle_turn_started(&mut self, payload: &TurnStartedEvent) {
        self.finish_current_turn();
        let mut turn = self
            .new_turn(Some(payload.turn_id.clone()))
            .with_status(TurnStatus::InProgress)
            .opened_explicitly();
        turn.collaboration_mode_kind = payload.collaboration_mode_kind;
        self.current_turn = Some(turn);
    }

    pub(super) fn handle_turn_complete(&mut self, payload: &TurnCompleteEvent) {
        let mark_completed = |status: &mut TurnStatus| {
            if matches!(*status, TurnStatus::Completed | TurnStatus::InProgress) {
                *status = TurnStatus::Completed;
            }
        };

        // Prefer an exact ID match from the active turn and then close it.
        if let Some(current_turn) = self
            .current_turn
            .as_mut()
            .filter(|turn| turn.id == payload.turn_id)
        {
            mark_completed(&mut current_turn.status);
            self.finish_current_turn();
            return;
        }

        if let Some(turn) = self
            .turns
            .iter_mut()
            .find(|turn| turn.id == payload.turn_id)
        {
            mark_completed(&mut turn.status);
            return;
        }

        // If the completion event cannot be matched, apply it to the active turn.
        if let Some(current_turn) = self.current_turn.as_mut() {
            mark_completed(&mut current_turn.status);
            self.finish_current_turn();
        }
    }
}
