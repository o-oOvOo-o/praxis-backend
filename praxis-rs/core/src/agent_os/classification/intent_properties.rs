use crate::agent_os::records::ActionIntentKind;

pub(in crate::agent_os) fn requires_write(intent: ActionIntentKind) -> bool {
    matches!(
        intent,
        ActionIntentKind::FileWrite
            | ActionIntentKind::Compile
            | ActionIntentKind::Test
            | ActionIntentKind::RunApp
            | ActionIntentKind::GitMutation
            | ActionIntentKind::UnknownRisky
    )
}

pub(in crate::agent_os) fn requires_dirty_audit(intent: ActionIntentKind) -> bool {
    requires_write(intent) || matches!(intent, ActionIntentKind::GitMutation)
}

pub(in crate::agent_os) fn requires_compile(intent: ActionIntentKind) -> bool {
    matches!(intent, ActionIntentKind::Compile | ActionIntentKind::Test)
}

pub(in crate::agent_os) fn requires_cpu_heavy(intent: ActionIntentKind) -> bool {
    matches!(
        intent,
        ActionIntentKind::Compile
            | ActionIntentKind::Test
            | ActionIntentKind::LongProcess
            | ActionIntentKind::UnknownRisky
    )
}
