use super::super::plugin::ProfileMatchContext;

pub(super) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.provider_identity_contains_any(&["openrouter"])
}
