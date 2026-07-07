use super::super::LiveEffectivePermissions;
use super::super::Session;
use super::super::SessionConfiguration;
use super::super::thread_permissions_from_session_configuration;
use praxis_protocol::models::PermissionProfile;

impl Session {
    pub(in crate::praxis) fn live_effective_permissions(&self) -> LiveEffectivePermissions {
        LiveEffectivePermissions::new(self.permission_controller.handle())
    }

    pub(in crate::praxis) fn publish_effective_permissions(
        &self,
        session_configuration: &SessionConfiguration,
    ) {
        self.permission_controller
            .replace(thread_permissions_from_session_configuration(
                session_configuration,
            ));
    }

    pub(crate) fn granted_permissions(&self) -> Option<PermissionProfile> {
        self.permission_controller.current().granted_permissions
    }

    pub(crate) fn grant_session_permissions(&self, permissions: PermissionProfile) {
        self.permission_controller
            .grant_session_permissions(permissions);
    }

    pub(crate) fn grant_turn_permissions(&self, permissions: PermissionProfile) {
        self.permission_controller
            .grant_turn_permissions(permissions);
    }

    pub(crate) fn clear_turn_permissions(&self) {
        self.permission_controller.clear_turn_permissions();
    }
}
