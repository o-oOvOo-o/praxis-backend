use crate::agent_os::model::CommandIntentPlan;
use crate::agent_os::model::ExecutionTicket;
use crate::path_scope::normalize_path_for_scope;

pub(in crate::agent_os) fn intent_plan_matches_ticket(
    plan: &CommandIntentPlan,
    ticket: &ExecutionTicket,
) -> bool {
    plan.thread_id == ticket.thread_id
        && plan.task_id == ticket.task_id
        && plan.intent == ticket.allowed_intent
        && plan.command_fingerprint == ticket.command_fingerprint
        && normalize_path_for_scope(plan.cwd.as_path())
            == normalize_path_for_scope(ticket.cwd.as_path())
}
