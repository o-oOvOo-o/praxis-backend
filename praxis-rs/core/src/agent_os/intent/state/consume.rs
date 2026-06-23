use chrono::DateTime;
use chrono::Utc;

use crate::agent_os::records::CommandIntentPlan;
use crate::agent_os::records::CommandIntentPlanStatus;
use crate::agent_os::records::ExecutionTicket;
use crate::agent_os::state::AgentOsState;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;

use super::matching::intent_plan_matches_ticket;

impl AgentOsState {
    pub(in crate::agent_os) fn consume_intent_plan_for_ticket(
        &mut self,
        ticket: &ExecutionTicket,
        intent_plan_id: &str,
        rejected_subject: &str,
        missing_subject: &str,
        now: DateTime<Utc>,
    ) -> PraxisResult<CommandIntentPlan> {
        let plan_snapshot = match self.intent_plans.get_mut(intent_plan_id) {
            Some(plan)
                if plan.status != CommandIntentPlanStatus::Pending || plan.expires_at <= now =>
            {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "{rejected_subject}: intent plan `{intent_plan_id}` is not pending"
                )));
            }
            Some(plan) if !intent_plan_matches_ticket(plan, ticket) => {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "{rejected_subject}: intent plan `{intent_plan_id}` does not match ticket action"
                )));
            }
            Some(plan) => {
                plan.status = CommandIntentPlanStatus::Consumed;
                plan.consumed_by_ticket_id = Some(ticket.ticket_id.clone());
                plan.clone()
            }
            None => {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "{missing_subject} references missing intent plan `{intent_plan_id}`"
                )));
            }
        };
        self.tickets
            .insert(ticket.ticket_id.clone(), ticket.clone());
        Ok(plan_snapshot)
    }
}
