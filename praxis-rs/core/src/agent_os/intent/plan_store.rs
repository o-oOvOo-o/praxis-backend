use crate::path_scope::normalize_path_for_scope;

use super::*;

impl AgentOs {
    pub(super) async fn insert_intent_plan(&self, plan: &CommandIntentPlan) {
        let superseded_plans = {
            let mut state = self.state.write().await;
            let normalized_cwd = normalize_path_for_scope(plan.cwd.as_path());
            let pending_ids = state
                .pending_intent_plan_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let mut superseded_plans = Vec::new();
            for plan_id in pending_ids {
                let Some(existing) = state.intent_plans.get_mut(&plan_id) else {
                    state.pending_intent_plan_ids.remove(&plan_id);
                    continue;
                };
                if existing.thread_id == plan.thread_id
                    && existing.task_id == plan.task_id
                    && existing.intent == plan.intent
                    && existing.command_fingerprint == plan.command_fingerprint
                    && normalize_path_for_scope(existing.cwd.as_path()) == normalized_cwd
                {
                    existing.status = CommandIntentPlanStatus::Rejected;
                    superseded_plans.push(existing.clone());
                }
            }
            for superseded in &superseded_plans {
                state.pending_intent_plan_ids.remove(&superseded.plan_id);
            }
            if plan.status == CommandIntentPlanStatus::Pending {
                state.pending_intent_plan_ids.insert(plan.plan_id.clone());
            }
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
            let pending_ids = state
                .pending_intent_plan_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let mut expired = Vec::new();
            for plan_id in pending_ids {
                let Some(plan) = state.intent_plans.get_mut(&plan_id) else {
                    state.pending_intent_plan_ids.remove(&plan_id);
                    continue;
                };
                if plan.expires_at <= now {
                    plan.status = CommandIntentPlanStatus::Expired;
                    expired.push(plan.clone());
                    state.pending_intent_plan_ids.remove(&plan_id);
                }
            }
            expired
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
