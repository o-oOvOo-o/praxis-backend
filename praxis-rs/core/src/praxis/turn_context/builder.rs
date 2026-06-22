use std::sync::Arc;

use serde_json::Value;

use crate::config::StartedNetworkProxy;

use super::super::Session;
use super::super::SessionConfiguration;
use super::TurnContext;
use super::mcp_sandbox;
use super::turn_skills;

impl Session {
    pub(in crate::praxis) async fn new_turn_from_configuration(
        &self,
        sub_id: String,
        session_configuration: SessionConfiguration,
        final_output_json_schema: Option<Option<Value>>,
        sandbox_policy_changed: bool,
    ) -> Arc<TurnContext> {
        self.publish_effective_permissions(&session_configuration);
        let per_turn_config = Self::build_per_turn_config(&session_configuration);
        mcp_sandbox::sync_for_turn(
            self,
            &session_configuration,
            &per_turn_config,
            sandbox_policy_changed,
        )
        .await;

        let model_info = self
            .services
            .models_manager
            .get_model_info(
                session_configuration.collaboration_mode.model(),
                &per_turn_config,
            )
            .await;
        let skills_outcome = turn_skills::load(self, &per_turn_config);
        let mut turn_context: TurnContext = Self::make_turn_context(
            self.conversation_id,
            Some(Arc::clone(&self.services.auth_manager)),
            &self.services.session_telemetry,
            session_configuration.provider.clone(),
            &session_configuration,
            self.services.user_shell.as_ref(),
            self.services.shell_zsh_path.as_ref(),
            self.services.main_execve_wrapper_exe.as_ref(),
            per_turn_config,
            model_info,
            &self.services.models_manager,
            &self.llm_runtime_catalog,
            self.services
                .network_proxy
                .as_ref()
                .map(StartedNetworkProxy::proxy),
            Arc::clone(&self.services.environment),
            sub_id,
            self.live_effective_permissions(),
            skills_outcome,
        );
        turn_context.realtime_active = self.conversation.running_state().await.is_some();

        if let Some(final_schema) = final_output_json_schema {
            turn_context.final_output_json_schema = final_schema;
        }
        let turn_context = Arc::new(turn_context);
        turn_context.turn_metadata_state.spawn_git_enrichment_task();
        turn_context
    }

    pub(crate) async fn maybe_emit_unknown_model_warning_for_turn(&self, tc: &TurnContext) {
        if tc.model_info.used_fallback_model_metadata {
            self.turn_event_emitter(tc)
                .warning(format!(
                    "Model metadata for `{}` not found. Defaulting to fallback metadata; this can degrade performance and cause issues.",
                    tc.model_info.slug
                ))
                .await;
        }
    }

    pub(crate) async fn new_default_turn(&self) -> Arc<TurnContext> {
        self.new_default_turn_with_sub_id(self.next_internal_sub_id())
            .await
    }

    pub(crate) async fn new_default_turn_with_sub_id(&self, sub_id: String) -> Arc<TurnContext> {
        let session_configuration = {
            let state = self.state.lock().await;
            state.session_configuration.clone()
        };
        self.new_turn_from_configuration(
            sub_id,
            session_configuration,
            /*final_output_json_schema*/ None,
            /*sandbox_policy_changed*/ false,
        )
        .await
    }
}
