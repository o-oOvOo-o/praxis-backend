use praxis_features::Feature;

use crate::praxis::Session;

pub(super) fn memory_tool_label(session: &Session) -> (&'static str, &'static str) {
    (
        "tmp_mem_enabled",
        if session.enabled(Feature::MemoryTool) {
            "true"
        } else {
            "false"
        },
    )
}
