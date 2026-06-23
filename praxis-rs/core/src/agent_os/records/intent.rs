use super::resource::ResourceRequirement;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum ActionIntentKind {
    ReadOnly,
    FileWrite,
    Harness,
    Test,
    Compile,
    RunApp,
    LongProcess,
    Network,
    Gpu,
    GitMutation,
    UnknownRisky,
}

impl ActionIntentKind {
    pub(in crate::agent_os) fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::FileWrite => "file_write",
            Self::Harness => "harness",
            Self::Test => "test",
            Self::Compile => "compile",
            Self::RunApp => "run_app",
            Self::LongProcess => "long_process",
            Self::Network => "network",
            Self::Gpu => "gpu",
            Self::GitMutation => "git_mutation",
            Self::UnknownRisky => "unknown_risky",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ActionIntent {
    pub(crate) kind: ActionIntentKind,
    pub(crate) confidence: f32,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) side_effects: Vec<String>,
    pub(crate) risk_level: String,
}
