use std::sync::Arc;

use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::protocol::Op;

use crate::praxis::Session;
use crate::praxis::SessionSettingsUpdate;
use crate::praxis::SteerInputError;

impl Session {
    pub(crate) async fn submit_user_input_or_turn(self: &Arc<Self>, sub_id: String, op: Op) {
        let (items, updates) = match op {
            Op::UserTurn {
                cwd,
                approval_policy,
                approvals_reviewer,
                sandbox_policy,
                model,
                model_provider,
                effort,
                summary,
                service_tier,
                final_output_json_schema,
                items,
                collaboration_mode,
                personality,
            } => {
                let collaboration_mode = collaboration_mode.or_else(|| {
                    Some(CollaborationMode {
                        mode: ModeKind::Default,
                        settings: Settings {
                            model: model.clone(),
                            reasoning_effort: effort,
                            developer_instructions: None,
                        },
                    })
                });
                (
                    items,
                    SessionSettingsUpdate {
                        cwd: Some(cwd),
                        approval_policy: Some(approval_policy),
                        approvals_reviewer,
                        sandbox_policy: Some(sandbox_policy),
                        windows_sandbox_level: None,
                        model_provider,
                        collaboration_mode,
                        reasoning_summary: summary,
                        service_tier,
                        final_output_json_schema: Some(final_output_json_schema),
                        personality,
                        app_gateway_client_name: None,
                    },
                )
            }
            Op::UserInput {
                items,
                final_output_json_schema,
            } => (
                items,
                SessionSettingsUpdate {
                    final_output_json_schema: Some(final_output_json_schema),
                    ..Default::default()
                },
            ),
            _ => unreachable!(),
        };

        let Ok(current_context) = self.new_turn_with_sub_id(sub_id.clone(), updates).await else {
            return;
        };
        self.maybe_emit_unknown_model_warning_for_turn(current_context.as_ref())
            .await;
        match self.steer_input(items.clone(), None).await {
            Ok(_) => {
                crate::auto_title::maybe_apply_provisional_title(self, &items).await;
                current_context.session_telemetry.user_prompt(&items);
            }
            Err(SteerInputError::NoActiveTurn(items)) => {
                crate::auto_title::maybe_apply_provisional_title(self, &items).await;
                current_context.session_telemetry.user_prompt(&items);
                self.refresh_mcp_servers_if_requested(&current_context)
                    .await;
                self.start_regular_task(Arc::clone(&current_context), items)
                    .await;
            }
            Err(err) => {
                self.raw_event_emitter(sub_id)
                    .error_event(err.to_error_event())
                    .await;
            }
        }
    }
}
