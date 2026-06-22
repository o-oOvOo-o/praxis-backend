use std::sync::Arc;

use crate::error::Result as PraxisResult;
use crate::praxis::Praxis;
use crate::praxis::PraxisSpawnArgs;
use crate::praxis::PraxisSpawnOk;

use super::super::super::ThreadManagerInner;
use super::super::super::ThreadSpawnResult;
use super::super::spawn_request::ThreadSpawnRequest;

impl ThreadManagerInner {
    pub(in crate::thread_manager::inner::spawn) async fn spawn_from_request(
        &self,
        request: ThreadSpawnRequest,
    ) -> PraxisResult<ThreadSpawnResult> {
        let watch_registration = self.skills_watcher.register_config(
            &request.config,
            self.skills_manager.as_ref(),
            self.plugins_manager.as_ref(),
        );
        let PraxisSpawnOk {
            praxis, thread_id, ..
        } = Praxis::spawn(PraxisSpawnArgs {
            config: request.config,
            auth_manager: request.auth_manager,
            models_manager: Arc::clone(&self.models_manager),
            environment_manager: Arc::clone(&self.environment_manager),
            skills_manager: Arc::clone(&self.skills_manager),
            plugins_manager: Arc::clone(&self.plugins_manager),
            mcp_manager: Arc::clone(&self.mcp_manager),
            skills_watcher: Arc::clone(&self.skills_watcher),
            conversation_history: request.initial_history,
            session_source: request.session_source,
            agent_control: request.agent_control,
            agent_os: Arc::clone(&self.agent_os),
            dynamic_tools: request.dynamic_tools,
            persist_extended_history: request.persist_extended_history,
            metrics_service_name: request.metrics_service_name,
            inherited_shell_snapshot: request.inherited_shell_snapshot,
            inherited_exec_policy: request.inherited_exec_policy,
            user_shell_override: request.user_shell_override,
            parent_trace: request.parent_trace,
        })
        .await?;
        self.finalize_thread_spawn(praxis, thread_id, watch_registration)
            .await
    }
}
