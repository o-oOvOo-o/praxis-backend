use tokio::sync::watch;

use crate::praxis::EffectivePermissions;
use crate::praxis::SessionConfiguration;

pub(super) struct SessionLiveChannels {
    pub(super) out_of_band_elicitation_paused: watch::Sender<bool>,
    pub(super) effective_permissions: watch::Sender<EffectivePermissions>,
}

pub(super) fn build(session_configuration: &SessionConfiguration) -> SessionLiveChannels {
    let (out_of_band_elicitation_paused, _out_of_band_elicitation_paused_rx) =
        watch::channel(false);
    let (effective_permissions, _effective_permissions_rx) = watch::channel(
        EffectivePermissions::from_session_configuration(session_configuration),
    );

    SessionLiveChannels {
        out_of_band_elicitation_paused,
        effective_permissions,
    }
}
