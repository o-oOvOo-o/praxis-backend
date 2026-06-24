use super::super::LiveEffectivePermissions;
use super::super::Session;
use super::super::SessionConfiguration;
use super::super::thread_permissions_from_session_configuration;

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
}
