use std::sync::Arc;

use praxis_protocol::protocol::PraxisErrorInfo;
use tracing::warn;

use crate::config::ConstraintResult;

use super::super::Session;
use super::super::SessionSettingsUpdate;
use super::TurnContext;

impl Session {
    pub(crate) async fn update_settings(
        &self,
        updates: SessionSettingsUpdate,
    ) -> ConstraintResult<()> {
        let (previous_configuration, mut updated) = {
            let state = self.state.lock().await;
            let previous_configuration = state.session_configuration.clone();
            let updated = match previous_configuration.apply(&updates) {
                Ok(updated) => updated,
                Err(err) => {
                    warn!("rejected session settings update: {err}");
                    return Err(err);
                }
            };
            (previous_configuration, updated)
        };

        if Self::prompt_route_update_needed(&previous_configuration, &updated, &updates) {
            self.refresh_model_base_instructions(&mut updated).await;
        }

        let previous_cwd = previous_configuration.cwd.clone();
        let next_cwd = updated.cwd.clone();
        let praxis_home = updated.praxis_home.clone();
        let session_source = updated.session_source.clone();
        {
            let mut state = self.state.lock().await;
            state.session_configuration = updated.clone();
        }
        self.publish_effective_permissions(&updated);

        self.maybe_refresh_shell_snapshot_for_cwd(
            &previous_cwd,
            &next_cwd,
            &praxis_home,
            &session_source,
        );

        Ok(())
    }

    pub(crate) async fn new_turn_with_sub_id(
        &self,
        sub_id: String,
        updates: SessionSettingsUpdate,
    ) -> ConstraintResult<Arc<TurnContext>> {
        let (
            previous_configuration,
            session_configuration,
            sandbox_policy_changed,
            previous_cwd,
            praxis_home,
            session_source,
        ) = {
            let state = self.state.lock().await;
            let previous_configuration = state.session_configuration.clone();
            match previous_configuration.apply(&updates) {
                Ok(next) => {
                    let sandbox_policy_changed =
                        previous_configuration.sandbox_policy != next.sandbox_policy;
                    let previous_cwd = previous_configuration.cwd.clone();
                    let praxis_home = next.praxis_home.clone();
                    let session_source = next.session_source.clone();
                    (
                        previous_configuration,
                        next,
                        sandbox_policy_changed,
                        previous_cwd,
                        praxis_home,
                        session_source,
                    )
                }
                Err(err) => {
                    drop(state);
                    self.raw_event_emitter(sub_id.clone())
                        .error(err.to_string(), Some(PraxisErrorInfo::BadRequest))
                        .await;
                    return Err(err);
                }
            }
        };

        let mut session_configuration = session_configuration;
        if Self::prompt_route_update_needed(
            &previous_configuration,
            &session_configuration,
            &updates,
        ) {
            self.refresh_model_base_instructions(&mut session_configuration)
                .await;
        }
        {
            let mut state = self.state.lock().await;
            state.session_configuration = session_configuration.clone();
        }
        self.publish_effective_permissions(&session_configuration);

        self.maybe_refresh_shell_snapshot_for_cwd(
            &previous_cwd,
            &session_configuration.cwd,
            &praxis_home,
            &session_source,
        );

        Ok(self
            .new_turn_from_configuration(
                sub_id,
                session_configuration,
                updates.final_output_json_schema,
                sandbox_policy_changed,
            )
            .await)
    }
}
