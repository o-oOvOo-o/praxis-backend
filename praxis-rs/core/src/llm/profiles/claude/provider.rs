use super::super::plugin::ProfileMatchContext;
use super::super::plugin::base_url;
use super::super::plugin::contains_any_text;
use crate::model_provider_info::WireApi;

pub(crate) fn matches(ctx: &ProfileMatchContext<'_>) -> bool {
    ctx.provider.wire_api == WireApi::Claude
        || contains_any_text(
            &[
                ctx.model_info.slug.as_str(),
                ctx.provider_id,
                ctx.provider.name.as_str(),
                base_url(ctx.provider),
            ],
            &["anthropic", "claude"],
        )
}
