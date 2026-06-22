use praxis_protocol::models::ResponseItem;

use crate::client_common::Prompt;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::ToolRouter;

use super::super::super::super::model_request::build_prompt;

pub(super) async fn build_provider_prompt(
    session: &Session,
    turn_context: &TurnContext,
    items: Vec<ResponseItem>,
    router: &ToolRouter,
) -> Prompt {
    let base_instructions = session.get_base_instructions().await;
    build_prompt(items, router, turn_context, base_instructions)
}
