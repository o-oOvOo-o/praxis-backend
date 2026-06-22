use std::sync::Arc;

use praxis_protocol::protocol::Op;

use super::super::skills_commands;
use super::super::submission_history;
use crate::config::Config;
use crate::praxis::Session;

pub(super) async fn handle(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String, op: Op) {
    match op {
        Op::AddToHistory { text } => {
            submission_history::add_to_history(sess, config, text).await;
        }
        Op::GetHistoryEntryRequest { offset, log_id } => {
            submission_history::get_history_entry_request(sess, config, sub_id, offset, log_id)
                .await;
        }
        Op::ListMcpTools => {
            sess.list_mcp_tools(config, sub_id).await;
        }
        Op::RefreshMcpServers { config } => {
            sess.queue_mcp_server_refresh(config).await;
        }
        Op::ReloadUserConfig => {
            sess.reload_user_config_layer().await;
        }
        Op::ListSkills { cwds, force_reload } => {
            skills_commands::list_skills(sess, sub_id, cwds, force_reload).await;
        }
        _ => {}
    }
}
