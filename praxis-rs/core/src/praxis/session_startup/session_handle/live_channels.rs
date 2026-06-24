use tokio::sync::watch;

pub(super) struct SessionLiveChannels {
    pub(super) out_of_band_elicitation_paused: watch::Sender<bool>,
}

pub(super) fn build() -> SessionLiveChannels {
    let (out_of_band_elicitation_paused, _out_of_band_elicitation_paused_rx) =
        watch::channel(false);

    SessionLiveChannels {
        out_of_band_elicitation_paused,
    }
}
