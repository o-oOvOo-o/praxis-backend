use super::super::plugin::ProfileMatchContext;
use crate::llm::ids::WireId;

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.wire_id_is(WireId::ClaudeMessages)
        || ctx.model_and_provider_identity_contains_any(&["anthropic", "claude"])
}
