use chrono::Utc;

use crate::agent_os::intent_state::intent_plan_matches_ticket;
use crate::agent_os::model::CommandIntentPlanStatus;
use crate::agent_os::model::ExecutionTicket;
use crate::agent_os::state::AgentOsState;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;

impl AgentOsState {
    pub(in crate::agent_os) fn validate_ticket(
        &self,
        ticket: &ExecutionTicket,
    ) -> PraxisResult<()> {
        if ticket.expires_at <= Utc::now() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "execution ticket `{}` has expired",
                ticket.ticket_id
            )));
        }
        if let Some(active) = self
            .active_coordinators
            .get(ticket.coordination_scope.as_str())
            && (ticket.coordinator_epoch != active.epoch
                || ticket.fencing_token != active.fencing_token)
        {
            return Err(PraxisErr::UnsupportedOperation(
                "execution ticket coordinator epoch is stale".to_string(),
            ));
        }
        for lease_id in &ticket.lease_ids {
            let lease = self.leases.get(lease_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing lease `{lease_id}`"
                ))
            })?;
            if lease.owner_thread_id != ticket.thread_id || lease.task_id != ticket.task_id {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references lease `{lease_id}` owned by another task or thread"
                )));
            }
        }
        if let Some(plan_id) = ticket.intent_plan_id.as_deref() {
            let plan = self.intent_plans.get(plan_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "execution ticket references missing intent plan `{plan_id}`"
                ))
            })?;
            if plan.status != CommandIntentPlanStatus::Consumed
                || plan.consumed_by_ticket_id.as_deref() != Some(ticket.ticket_id.as_str())
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket intent plan `{plan_id}` was not consumed by this ticket"
                )));
            }
            if !intent_plan_matches_ticket(plan, ticket) {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "execution ticket intent plan `{plan_id}` does not match ticket action"
                )));
            }
        }
        Ok(())
    }
}
