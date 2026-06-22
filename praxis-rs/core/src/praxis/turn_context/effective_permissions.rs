use super::super::EffectivePermissions;
use super::super::LiveEffectivePermissions;
use super::super::Session;
use super::super::SessionConfiguration;

impl Session {
    pub(in crate::praxis) fn live_effective_permissions(&self) -> LiveEffectivePermissions {
        LiveEffectivePermissions::new(self.effective_permissions.subscribe())
    }

    pub(in crate::praxis) fn publish_effective_permissions(
        &self,
        session_configuration: &SessionConfiguration,
    ) {
        self.effective_permissions
            .send_replace(EffectivePermissions::from_session_configuration(
                session_configuration,
            ));
    }
}
