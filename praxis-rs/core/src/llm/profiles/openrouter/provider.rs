use super::super::plugin::ProfileMatchContext;
use super::super::plugin::base_url;
use super::super::plugin::contains_any_text;

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    contains_any_text(
        &[
            ctx.provider_id,
            ctx.provider.name.as_str(),
            base_url(ctx.provider),
        ],
        &["openrouter"],
    )
}
