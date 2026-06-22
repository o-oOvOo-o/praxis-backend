use std::sync::Arc;

use praxis_protocol::protocol::Op;

use super::super::memory_commands;
use super::super::review;
use crate::config::Config;
use crate::praxis::Session;

pub(super) async fn handle(sess: &Arc<Session>, config: &Arc<Config>, sub_id: String, op: Op) {
    match op {
        Op::Undo => {
            sess.start_undo_task(sub_id).await;
        }
        Op::Compact => {
            sess.start_compact_task(sub_id).await;
        }
        Op::DropMemories => {
            memory_commands::drop_memories(sess, config, sub_id).await;
        }
        Op::UpdateMemories => {
            memory_commands::update_memories(sess, config, sub_id).await;
        }
        Op::ThreadRollback { num_turns } => {
            sess.rollback_thread(sub_id, num_turns).await;
        }
        Op::SetThreadName { name } => {
            sess.set_thread_name_from_user(sub_id, name).await;
        }
        Op::RunUserShellCommand { command } => {
            sess.run_user_shell_command_task(sub_id, command).await;
        }
        Op::Review { review_request } => {
            review::start_review(sess, config, sub_id, review_request).await;
        }
        _ => {}
    }
}
