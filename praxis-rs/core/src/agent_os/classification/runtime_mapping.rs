use crate::agent_os::model::ActionIntentKind;
use crate::agent_os::model::ArtifactType;
use crate::agent_os::process::process_runtime_kind;

pub(in crate::agent_os) fn runtime_kind_for_intent(intent: ActionIntentKind) -> &'static str {
    match intent {
        ActionIntentKind::RunApp | ActionIntentKind::LongProcess => {
            process_runtime_kind::LONG_PROCESS
        }
        ActionIntentKind::Compile | ActionIntentKind::Test | ActionIntentKind::Harness => {
            process_runtime_kind::COMMAND
        }
        ActionIntentKind::Gpu => process_runtime_kind::GPU_COMMAND,
        ActionIntentKind::Network => process_runtime_kind::NETWORK_COMMAND,
        _ => process_runtime_kind::COMMAND,
    }
}

pub(in crate::agent_os) fn artifact_type_for_intent(intent: ActionIntentKind) -> ArtifactType {
    match intent {
        ActionIntentKind::Compile | ActionIntentKind::Test | ActionIntentKind::Harness => {
            ArtifactType::CompileLog
        }
        _ => ArtifactType::CommandLog,
    }
}
