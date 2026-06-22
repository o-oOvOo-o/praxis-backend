use std::path::Path;

use chrono::Utc;
use praxis_protocol::ThreadId;

use crate::agent_os::model::ActionIntentKind;
use crate::agent_os::model::CommandIntentPlan;
use crate::agent_os::model::CommandIntentPlanStatus;
use crate::agent_os::state::AgentOsState;
use crate::path_scope::normalize_path_for_scope;

impl AgentOsState {
    pub(in crate::agent_os) fn find_matching_intent_plan(
        &self,
        thread_id: ThreadId,
        task_id: &str,
        intent: ActionIntentKind,
        command_fingerprint: &str,
        cwd: &Path,
    ) -> Option<&CommandIntentPlan> {
        let cwd = normalize_path_for_scope(cwd);
        let now = Utc::now();
        self.intent_plans
            .values()
            .filter(|plan| plan.status == CommandIntentPlanStatus::Pending)
            .filter(|plan| plan.expires_at > now)
            .filter(|plan| plan.thread_id == thread_id)
            .filter(|plan| plan.task_id == task_id)
            .filter(|plan| plan.intent == intent)
            .filter(|plan| plan.command_fingerprint == command_fingerprint)
            .filter(|plan| normalize_path_for_scope(plan.cwd.as_path()) == cwd)
            .max_by_key(|plan| plan.created_at)
    }
}
