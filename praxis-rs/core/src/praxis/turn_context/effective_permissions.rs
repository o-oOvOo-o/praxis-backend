use super::super::LiveEffectivePermissions;
use super::super::Session;
use super::super::SessionConfiguration;
use praxis_protocol::models::PermissionProfile;

impl Session {
    pub(in crate::praxis) fn live_effective_permissions(&self) -> LiveEffectivePermissions {
        self.permission_ledger.live_effective_permissions()
    }

    pub(in crate::praxis) fn publish_effective_permissions(
        &self,
        session_configuration: &SessionConfiguration,
    ) {
        self.permission_ledger
            .publish_session_configuration(session_configuration);
    }

    pub(crate) fn granted_permissions(&self) -> Option<PermissionProfile> {
        self.permission_ledger.granted_permissions()
    }

    pub(crate) fn grant_session_permissions(&self, permissions: PermissionProfile) {
        self.permission_ledger
            .grant_session_permissions(permissions);
    }

    pub(crate) fn grant_turn_permissions(&self, permissions: PermissionProfile) {
        self.permission_ledger.grant_turn_permissions(permissions);
    }

    pub(crate) fn clear_turn_permissions(&self) {
        self.permission_ledger.clear_turn_permissions();
    }
}
