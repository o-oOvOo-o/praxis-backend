use super::super::Session;
use super::super::TurnContext;

mod collaboration;
mod model;
mod permissions;

pub(super) fn build_context(
    sess: &Session,
    turn_context: &TurnContext,
    initial_prompt_items: Vec<praxis_loop::model::PromptItem>,
) -> praxis_loop::TurnContext {
    let mut ctx = praxis_loop::TurnContext::new(
        praxis_loop::ids::TurnId::new(turn_context.sub_id.clone()),
        praxis_loop::ids::ThreadId::new(sess.conversation_id.to_string()),
        praxis_loop::ids::TraceId::new(turn_context.trace_id.clone().unwrap_or_default()),
        model::build_model_spec(turn_context),
    );
    ctx.reasoning = turn_context
        .reasoning_effort
        .clone()
        .map(|reasoning_effort| reasoning_effort.to_string());
    ctx.service_tier = turn_context
        .config
        .service_tier
        .map(|service_tier| service_tier.to_string());
    ctx.permissions = permissions::build_permissions(turn_context);
    ctx.collaboration_mode = collaboration::build_collaboration_mode(turn_context);
    ctx.cwd = Some(turn_context.cwd.to_path_buf());
    ctx.features = praxis_loop::context::TurnFeatures {
        streaming: true,
        tool_calls: true,
    };
    ctx.initial_prompt_items = initial_prompt_items;
    ctx
}
