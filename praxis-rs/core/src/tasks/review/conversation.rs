use std::sync::Arc;

use praxis_features::Feature;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use crate::config::Constrained;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::praxis_delegate::run_praxis_thread_one_shot;

pub(super) async fn start(
    session: Arc<Session>,
    ctx: Arc<TurnContext>,
    input: Vec<UserInput>,
    cancellation_token: CancellationToken,
) -> Option<async_channel::Receiver<Event>> {
    let config = ctx.config.clone();
    let mut sub_agent_config = config.as_ref().clone();
    if let Err(err) = sub_agent_config
        .web_search_mode
        .set(WebSearchMode::Disabled)
    {
        panic!("by construction Constrained<WebSearchMode> must always support Disabled: {err}");
    }
    let _ = sub_agent_config.features.disable(Feature::SpawnCsv);
    let _ = sub_agent_config.features.disable(Feature::Collab);
    let auth_manager = Arc::clone(&session.services.auth_manager);
    let models_manager = Arc::clone(&session.services.models_manager);

    sub_agent_config.base_instructions = Some(crate::REVIEW_PROMPT.to_string());
    sub_agent_config.permissions.approval_policy = Constrained::allow_only(AskForApproval::Never);

    let model = config
        .review_model
        .clone()
        .unwrap_or_else(|| ctx.model_info.slug.clone());
    sub_agent_config.model = Some(model);
    (run_praxis_thread_one_shot(
        sub_agent_config,
        auth_manager,
        models_manager,
        input,
        session,
        ctx.clone(),
        cancellation_token,
        SubAgentSource::Review,
        /*final_output_json_schema*/ None,
        /*initial_history*/ None,
    )
    .await)
        .ok()
        .map(|io| io.rx_event)
}
