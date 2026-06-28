use std::sync::Arc;

use praxis_protocol::permissions::FileSystemSandboxPolicy;
use praxis_protocol::permissions::NetworkSandboxPolicy;
use praxis_utils_absolute_path::AbsolutePathBuf;
use tracing::warn;

use crate::config::ConstraintError;
use crate::config::ConstraintResult;
use crate::config_loader::RequirementSource;
use crate::path_utils::normalize_for_native_workdir;

use super::types::SessionConfiguration;
use super::types::SessionSettingsUpdate;

impl SessionConfiguration {
    pub(crate) fn apply(&self, updates: &SessionSettingsUpdate) -> ConstraintResult<Self> {
        let mut next_configuration = self.clone();
        let file_system_policy_matches_legacy = self.file_system_sandbox_policy
            == FileSystemSandboxPolicy::from_sandbox_policy(
                self.sandbox_policy.get(),
                &self.cwd,
            );
        if let Some(model_provider_id) = updates.model_provider.as_ref() {
            if model_provider_id.is_empty() {
                return Err(ConstraintError::empty_field("model_provider"));
            }

            let mut allowed_model_providers = next_configuration
                .original_config_do_not_use
                .model_providers
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            allowed_model_providers.sort();

            let provider = next_configuration
                .original_config_do_not_use
                .model_providers
                .get(model_provider_id)
                .cloned()
                .ok_or_else(|| ConstraintError::InvalidValue {
                    field_name: "model_provider",
                    candidate: model_provider_id.clone(),
                    allowed: format!("{allowed_model_providers:?}"),
                    requirement_source: RequirementSource::Unknown,
                })?;

            next_configuration.provider = provider.clone();

            let mut config = (*next_configuration.original_config_do_not_use).clone();
            config.model_provider_id = model_provider_id.clone();
            config.model_provider = provider;
            next_configuration.original_config_do_not_use = Arc::new(config);
        }
        if let Some(collaboration_mode) = updates.collaboration_mode.clone() {
            next_configuration.collaboration_mode = collaboration_mode;
        }
        if let Some(summary) = updates.reasoning_summary {
            next_configuration.model_reasoning_summary = Some(summary);
        }
        if let Some(service_tier) = updates.service_tier {
            next_configuration.service_tier = service_tier;
        }
        if let Some(personality) = updates.personality {
            next_configuration.personality = Some(personality);
        }
        if let Some(approval_policy) = updates.approval_policy {
            next_configuration.approval_policy.set(approval_policy)?;
        }
        if let Some(approvals_reviewer) = updates.approvals_reviewer {
            next_configuration.approvals_reviewer = approvals_reviewer;
        }
        let mut sandbox_policy_changed = false;
        if let Some(sandbox_policy) = updates.sandbox_policy.clone() {
            next_configuration.sandbox_policy.set(sandbox_policy)?;
            next_configuration.network_sandbox_policy =
                NetworkSandboxPolicy::from(next_configuration.sandbox_policy.get());
            sandbox_policy_changed = true;
        }
        if let Some(windows_sandbox_level) = updates.windows_sandbox_level {
            next_configuration.windows_sandbox_level = windows_sandbox_level;
        }

        let absolute_cwd = updates
            .cwd
            .as_ref()
            .map(|cwd| {
                AbsolutePathBuf::relative_to_current_dir(normalize_for_native_workdir(
                    cwd.as_path(),
                ))
                .unwrap_or_else(|e| {
                    warn!("failed to normalize update cwd: {cwd:?}: {e}");
                    self.cwd.clone()
                })
            })
            .unwrap_or_else(|| self.cwd.clone());

        let cwd_changed = absolute_cwd.as_path() != self.cwd.as_path();
        next_configuration.cwd = absolute_cwd;
        if sandbox_policy_changed || (cwd_changed && file_system_policy_matches_legacy) {
            next_configuration.file_system_sandbox_policy =
                FileSystemSandboxPolicy::from_sandbox_policy(
                    next_configuration.sandbox_policy.get(),
                    &next_configuration.cwd,
                );
        }
        if let Some(app_gateway_client_name) = updates.app_gateway_client_name.clone() {
            next_configuration.app_gateway_client_name = Some(app_gateway_client_name);
        }
        Ok(next_configuration)
    }
}
