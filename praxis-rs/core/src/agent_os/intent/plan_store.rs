use crate::path_scope::normalize_path_for_scope;

use super::*;

impl AgentOs {
    pub(super) async fn insert_intent_plan(&self, plan: &CommandIntentPlan) {
        let superseded_plans = {
            let mut state = self.state.write().await;
            let normalized_cwd = normalize_path_for_scope(plan.cwd.as_path());
            let superseded_plans = state
                .intent_plans
                .values_mut()
                .filter(|existing| existing.status == CommandIntentPlanStatus::Pending)
                .filter(|existing| existing.thread_id == plan.thread_id)
                .filter(|existing| existing.task_id == plan.task_id.as_str())
                .filter(|existing| existing.intent == plan.intent)
                .filter(|existing| {
                    existing.command_fingerprint == plan.command_fingerprint.as_str()
                })
                .filter(|existing| {
                    normalize_path_for_scope(existing.cwd.as_path()) == normalized_cwd
                })
                .map(|existing| {
                    existing.status = CommandIntentPlanStatus::Rejected;
                    existing.clone()
                })
                .collect::<Vec<_>>();
            state
                .intent_plans
                .insert(plan.plan_id.clone(), plan.clone());
            superseded_plans
        };
        for superseded in superseded_plans {
            self.persist_intent_plan_snapshot(&superseded).await;
            self.record_event(
                "command_intent_plan_superseded",
                Some(superseded.thread_id),
                Some(superseded.task_id.clone()),
                None,
                json!({
                    "plan_id": &superseded.plan_id,
                    "replaced_by_plan_id": &plan.plan_id,
                    "intent": superseded.intent.as_str(),
                }),
            )
            .await;
        }
    }

    pub(in crate::agent_os) async fn expire_intent_plans(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            state
                .intent_plans
                .values_mut()
                .filter(|plan| plan.status == CommandIntentPlanStatus::Pending)
                .filter(|plan| plan.expires_at <= now)
                .map(|plan| {
                    plan.status = CommandIntentPlanStatus::Expired;
                    plan.clone()
                })
                .collect::<Vec<_>>()
        };
        for plan in expired {
            self.persist_intent_plan_snapshot(&plan).await;
            self.record_event(
                "command_intent_plan_expired",
                Some(plan.thread_id),
                Some(plan.task_id.clone()),
                None,
                json!({
                    "plan_id": &plan.plan_id,
                    "intent": plan.intent.as_str(),
                    "expires_at": plan.expires_at.to_rfc3339(),
                }),
            )
            .await;
        }
    }
}
