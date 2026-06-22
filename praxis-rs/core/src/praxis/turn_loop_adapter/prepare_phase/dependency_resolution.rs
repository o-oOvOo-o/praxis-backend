use std::sync::Arc;

use praxis_features::Feature;
use tokio_util::sync::CancellationToken;

use crate::collect_env_var_dependencies;
use crate::config::Config;
use crate::mcp_skill_dependencies::maybe_prompt_and_install_mcp_dependencies;
use crate::resolve_skill_dependencies_for_turn;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::mentions::TurnPrepareMentions;

pub(super) async fn resolve_prepare_dependencies(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    config: &Config,
    mentions: &TurnPrepareMentions,
    cancellation_token: &CancellationToken,
) {
    if config
        .features
        .enabled(Feature::SkillEnvVarDependencyPrompt)
    {
        let env_var_dependencies = collect_env_var_dependencies(&mentions.mentioned_skills);
        resolve_skill_dependencies_for_turn(sess, turn_context, &env_var_dependencies).await;
    }

    maybe_prompt_and_install_mcp_dependencies(
        sess.as_ref(),
        turn_context.as_ref(),
        cancellation_token,
        &mentions.mentioned_skills,
    )
    .await;
}
