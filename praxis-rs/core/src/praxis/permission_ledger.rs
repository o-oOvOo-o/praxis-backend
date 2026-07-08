use praxis_protocol::ThreadId;
use praxis_protocol::models::PermissionProfile;
use praxis_system_plugin_approval_control::PermissionController;
use praxis_system_plugin_approval_control::ThreadPermissionState;

use super::LiveEffectivePermissions;
use super::SessionConfiguration;
use super::thread_permissions_from_session_configuration;

#[derive(Debug, Clone)]
pub(crate) struct PermissionLedger {
    controller: PermissionController,
}

impl PermissionLedger {
    pub(crate) fn new(initial: ThreadPermissionState) -> Self {
        Self {
            controller: PermissionController::new(initial),
        }
    }

    pub(crate) fn from_session_configuration(
        thread_id: &ThreadId,
        session_configuration: &SessionConfiguration,
    ) -> Self {
        let initial = thread_permissions_from_session_configuration(session_configuration)
            .with_thread_id(thread_id.to_string());
        Self::new(initial)
    }

    pub(crate) fn live_effective_permissions(&self) -> LiveEffectivePermissions {
        LiveEffectivePermissions::new(self.controller.handle())
    }

    pub(crate) fn publish_session_configuration(
        &self,
        session_configuration: &SessionConfiguration,
    ) {
        self.controller
            .replace(thread_permissions_from_session_configuration(
                session_configuration,
            ));
    }

    pub(crate) fn granted_permissions(&self) -> Option<PermissionProfile> {
        self.controller.current().granted_permissions
    }

    pub(crate) fn grant_session_permissions(&self, permissions: PermissionProfile) {
        self.controller.grant_session_permissions(permissions);
    }

    pub(crate) fn grant_turn_permissions(&self, permissions: PermissionProfile) {
        self.controller.grant_turn_permissions(permissions);
    }

    pub(crate) fn clear_turn_permissions(&self) {
        self.controller.clear_turn_permissions();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_protocol::config_types::ApprovalsReviewer;
    use praxis_protocol::config_types::WindowsSandboxLevel;
    use praxis_protocol::models::FileSystemPermissions;
    use praxis_protocol::models::NetworkPermissions;
    use praxis_protocol::permissions::FileSystemSandboxPolicy;
    use praxis_protocol::permissions::NetworkSandboxPolicy;
    use praxis_protocol::protocol::AskForApproval;
    use praxis_protocol::protocol::SandboxPolicy;
    use praxis_system_plugin_approval_control::PermissionStateSource;
    use praxis_utils_absolute_path::AbsolutePathBuf;

    fn ledger() -> PermissionLedger {
        let sandbox_policy = SandboxPolicy::new_read_only_policy();
        let file_system_sandbox_policy = FileSystemSandboxPolicy::from(&sandbox_policy);
        PermissionLedger::new(ThreadPermissionState::new(
            Some("thread".to_string()),
            PermissionStateSource::Config,
            AskForApproval::OnRequest,
            ApprovalsReviewer::User,
            sandbox_policy,
            file_system_sandbox_policy,
            NetworkSandboxPolicy::Enabled,
            WindowsSandboxLevel::Disabled,
        ))
    }

    fn network_permissions() -> PermissionProfile {
        PermissionProfile {
            network: Some(NetworkPermissions {
                enabled: Some(true),
            }),
            ..Default::default()
        }
    }

    fn write_permissions() -> PermissionProfile {
        let cwd = std::env::current_dir().expect("current dir");
        PermissionProfile {
            file_system: Some(FileSystemPermissions {
                read: None,
                write: Some(vec![
                    AbsolutePathBuf::from_absolute_path(cwd.as_path()).expect("absolute cwd"),
                ]),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn live_permissions_are_projected_from_the_ledger_owner() {
        let ledger = ledger();
        let live_permissions = ledger.live_effective_permissions();
        let permissions = network_permissions();

        assert_eq!(live_permissions.snapshot().granted_permissions, None);

        ledger.grant_turn_permissions(permissions.clone());

        assert_eq!(
            live_permissions.snapshot().granted_permissions,
            Some(permissions)
        );
    }

    #[test]
    fn clearing_turn_permissions_preserves_session_grants() {
        let ledger = ledger();
        let session_permissions = network_permissions();
        let turn_permissions = write_permissions();

        ledger.grant_session_permissions(session_permissions.clone());
        ledger.grant_turn_permissions(turn_permissions.clone());

        let merged = ledger.granted_permissions().expect("merged permissions");
        assert_eq!(merged.network, session_permissions.network);
        assert_eq!(merged.file_system, turn_permissions.file_system);

        ledger.clear_turn_permissions();

        assert_eq!(ledger.granted_permissions(), Some(session_permissions));
    }
}
