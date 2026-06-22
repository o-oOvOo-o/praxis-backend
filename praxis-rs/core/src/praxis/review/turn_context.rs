use std::sync::Arc;

use praxis_protocol::user_input::UserInput;

use crate::config::Config;

use super::super::Session;
use super::super::TurnContext;

mod assembly;
mod model;
mod tools;

pub(super) struct ReviewTurnContext {
    pub(super) context: Arc<TurnContext>,
    pub(super) input: Vec<UserInput>,
}

pub(super) async fn build(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    parent_turn_context: &Arc<TurnContext>,
    review_turn_id: String,
    review_prompt: String,
) -> ReviewTurnContext {
    let model = model::select_review_model(config, parent_turn_context);
    let review_model_info = model::load_review_model_info(sess, config, &model).await;
    let review_features = model::review_features(sess);
    let review_web_search_mode = model::review_web_search_mode();
    let tools_config = tools::build(
        sess,
        config,
        parent_turn_context,
        &review_model_info,
        &review_features,
        review_web_search_mode,
    )
    .await;
    let per_turn_config =
        model::build_per_turn_config(config, &model, review_features, review_web_search_mode);
    let context = assembly::build(assembly::ReviewTurnContextAssemblyInput {
        session: sess,
        parent_turn_context,
        model,
        model_info: review_model_info,
        per_turn_config,
        tools_config,
        review_turn_id,
    });

    ReviewTurnContext {
        context,
        input: vec![UserInput::Text {
            text: review_prompt,
            text_elements: Vec::new(),
        }],
    }
}
