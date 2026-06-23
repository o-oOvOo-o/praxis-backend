use crate::agent_os::records::CommandIntentPlan;
use crate::agent_os::records::ResourceRequirement;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsIntentPlanSummary {
    plan_id: String,
    task_id: String,
    thread_id: String,
    intent: String,
    confidence: f32,
    command_fingerprint: String,
    cwd: String,
    required_capabilities: Vec<String>,
    required_resources: Vec<String>,
    risk_level: String,
    status: String,
    consumed_by_ticket_id: Option<String>,
    created_at: String,
    expires_at: String,
}

impl From<CommandIntentPlan> for AgentOsIntentPlanSummary {
    fn from(plan: CommandIntentPlan) -> Self {
        Self {
            plan_id: plan.plan_id,
            task_id: plan.task_id,
            thread_id: plan.thread_id.to_string(),
            intent: format!("{:?}", plan.intent),
            confidence: plan.confidence,
            command_fingerprint: plan.command_fingerprint,
            cwd: plan.cwd.display().to_string(),
            required_capabilities: plan.required_capabilities,
            required_resources: plan
                .required_resources
                .iter()
                .map(ResourceRequirement::key)
                .collect(),
            risk_level: plan.risk_level,
            status: format!("{:?}", plan.status),
            consumed_by_ticket_id: plan.consumed_by_ticket_id,
            created_at: plan.created_at.to_rfc3339(),
            expires_at: plan.expires_at.to_rfc3339(),
        }
    }
}
