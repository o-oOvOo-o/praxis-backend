#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompactExecutionPolicy {
    RemoteResponses,
    LocalPrompt,
}

impl CompactExecutionPolicy {
    pub(crate) fn telemetry_kind(self) -> &'static str {
        match self {
            Self::RemoteResponses => "remote",
            Self::LocalPrompt => "local",
        }
    }
}
