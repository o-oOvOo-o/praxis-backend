use super::provider_projection::ModelOutputObservation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModelStreamProgress {
    NoModelOutput,
    ModelOutputStarted,
}

impl ModelStreamProgress {
    pub(super) const fn has_model_output(self) -> bool {
        matches!(self, Self::ModelOutputStarted)
    }
}

#[derive(Default)]
pub(super) struct ProviderStreamRunState {
    retries: u64,
    emitted_model_event: bool,
}

impl ProviderStreamRunState {
    pub(super) fn retry_count_mut(&mut self) -> &mut u64 {
        &mut self.retries
    }

    pub(super) fn model_stream_progress(&self) -> ModelStreamProgress {
        if self.emitted_model_event {
            ModelStreamProgress::ModelOutputStarted
        } else {
            ModelStreamProgress::NoModelOutput
        }
    }

    pub(super) fn observe_model_output(&mut self, observation: ModelOutputObservation) {
        self.emitted_model_event |= observation.as_bool();
    }
}
