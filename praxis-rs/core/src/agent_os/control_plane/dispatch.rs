use serde_json::json;

use crate::agent_os::control_plane::AgentTaskDispatchRequest;
use crate::agent_os::control_plane::AgentTaskDispatchResult;
use crate::agent_os::instance::AgentOs;
use crate::agent_os::model::ResourceRequirement;
use crate::agent_os::model::RuntimeCommandType;
use crate::agent_os::model::TaskCreateRequest;
use crate::error::Result as PraxisResult;

impl AgentOs {
    pub(crate) async fn dispatch_task(
        &self,
        request: AgentTaskDispatchRequest,
    ) -> PraxisResult<AgentTaskDispatchResult> {
        self.ensure_inter_thread_message_allowed(
            request.from_thread_id,
            request.to_thread_id,
            /*require_active_dispatcher*/ true,
        )
        .await?;

        let task_id = self
            .create_task(TaskCreateRequest {
                objective: request.objective.clone(),
                scope: request.scope.clone(),
                constraints: request.constraints.clone(),
                acceptance_criteria: request.acceptance_criteria.clone(),
                artifact_refs: request.artifact_refs.clone(),
                priority: request.priority,
                assigned_thread_id: Some(request.to_thread_id),
                required_capabilities: request.required_capabilities.clone(),
                required_resources: request.required_resources.clone(),
                token_budget: request.token_budget,
                exploratory: request.exploratory,
                created_by: request.from_thread_id,
            })
            .await?;
        self.assign_task(task_id.as_str(), request.to_thread_id)
            .await?;

        let payload = json!({
            "objective": &request.objective,
            "prompt": &request.prompt,
            "scope": &request.scope,
            "constraints": &request.constraints,
            "acceptance_criteria": &request.acceptance_criteria,
            "artifact_refs": &request.artifact_refs,
            "required_capabilities": &request.required_capabilities,
            "required_resources": request.required_resources.iter().map(ResourceRequirement::key).collect::<Vec<_>>(),
            "token_budget": request.token_budget,
            "priority": request.priority,
            "exploratory": request.exploratory,
            "interrupt": request.interrupt,
        });
        let runtime_command = self
            .issue_runtime_command(
                request.from_thread_id,
                request.to_thread_id,
                RuntimeCommandType::AssignTask,
                Some(task_id.clone()),
                payload.clone(),
            )
            .await?;

        Ok(AgentTaskDispatchResult {
            task_id,
            runtime_command,
            runtime_command_payload: payload,
        })
    }
}
