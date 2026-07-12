use praxis_protocol::protocol::TurnContextItem;
use praxis_protocol::protocol::TurnContextNetworkItem;

use super::TurnContext;

impl TurnContext {
    pub(crate) fn to_turn_context_item(&self) -> TurnContextItem {
        let permissions = self.effective_permissions();
        TurnContextItem {
            turn_id: Some(self.sub_id.clone()),
            trace_id: self.trace_id.clone(),
            cwd: self.cwd.to_path_buf(),
            current_date: self.current_date.clone(),
            timezone: self.timezone.clone(),
            approval_policy: permissions.approval_policy.value(),
            sandbox_policy: permissions.sandbox_policy.get().clone(),
            network: self.turn_context_network_item(),
            model: self.model_info.slug.clone(),
            personality: self.personality,
            collaboration_mode: Some(self.collaboration_mode.clone()),
            realtime_active: Some(self.realtime_active),
            effort: self.reasoning_effort.clone(),
            summary: self.reasoning_summary,
            user_instructions: self.user_instructions.clone(),
            developer_instructions: self.developer_instructions.clone(),
            final_output_json_schema: self.final_output_json_schema.clone(),
            truncation_policy: Some(self.truncation_policy),
        }
    }

    fn turn_context_network_item(&self) -> Option<TurnContextNetworkItem> {
        let network = self
            .config
            .config_layer_stack
            .requirements()
            .network
            .as_ref()?;
        Some(TurnContextNetworkItem {
            allowed_domains: network
                .domains
                .as_ref()
                .and_then(praxis_config::NetworkDomainPermissionsToml::allowed_domains)
                .unwrap_or_default(),
            denied_domains: network
                .domains
                .as_ref()
                .and_then(praxis_config::NetworkDomainPermissionsToml::denied_domains)
                .unwrap_or_default(),
        })
    }
}
