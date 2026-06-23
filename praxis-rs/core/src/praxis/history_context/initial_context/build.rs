use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::contextual_user_sections::build_contextual_user_sections;
use super::developer_sections::build_developer_sections;
use super::output_items::build_initial_context_items;
use super::state_snapshot::InitialContextStateSnapshot;

impl Session {
    pub(crate) async fn build_initial_context(
        &self,
        turn_context: &TurnContext,
    ) -> Vec<praxis_protocol::models::ResponseItem> {
        let snapshot = InitialContextStateSnapshot::capture(self).await;
        let separate_guardian_developer_message =
            crate::guardian::is_guardian_reviewer_source(&snapshot.session_source);
        let developer_sections = build_developer_sections(
            self,
            turn_context,
            &snapshot,
            separate_guardian_developer_message,
        )
        .await;
        let contextual_user_sections = build_contextual_user_sections(self, turn_context).await;
        build_initial_context_items(
            developer_sections,
            contextual_user_sections,
            separate_guardian_developer_message,
            turn_context.developer_instructions.as_deref(),
        )
    }
}
