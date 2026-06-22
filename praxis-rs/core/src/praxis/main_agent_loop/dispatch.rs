use std::sync::Arc;

use praxis_protocol::protocol::Op;

use crate::config::Config;
use crate::praxis::Session;

use super::command_ops;
use super::override_turn_context;
use super::realtime_ops;
use super::response_ops;
use super::task_ops;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DispatchOutcome {
    Continue,
    Exit,
}

pub(super) async fn dispatch_op(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    sub_id: String,
    op: Op,
) -> DispatchOutcome {
    match op {
        Op::Interrupt => {
            sess.interrupt_task().await;
            DispatchOutcome::Continue
        }
        Op::CleanBackgroundTerminals => {
            sess.close_unified_exec_processes().await;
            DispatchOutcome::Continue
        }
        Op::RealtimeConversationStart(params) => {
            realtime_ops::start(sess, sub_id, params).await;
            DispatchOutcome::Continue
        }
        Op::RealtimeConversationAudio(params) => {
            realtime_ops::audio(sess, sub_id, params).await;
            DispatchOutcome::Continue
        }
        Op::RealtimeConversationText(params) => {
            realtime_ops::text(sess, sub_id, params).await;
            DispatchOutcome::Continue
        }
        Op::RealtimeConversationClose => {
            realtime_ops::close(sess, sub_id).await;
            DispatchOutcome::Continue
        }
        op @ Op::OverrideTurnContext { .. } => {
            override_turn_context::handle(sess, sub_id, op).await;
            DispatchOutcome::Continue
        }
        op @ (Op::UserInput { .. } | Op::UserTurn { .. }) => {
            sess.submit_user_input_or_turn(sub_id, op).await;
            DispatchOutcome::Continue
        }
        Op::InterAgentCommunication { communication } => {
            sess.receive_inter_agent_communication(sub_id, communication)
                .await;
            DispatchOutcome::Continue
        }
        op @ (Op::ExecApproval { .. }
        | Op::PatchApproval { .. }
        | Op::UserInputAnswer { .. }
        | Op::RequestPermissionsResponse { .. }
        | Op::DynamicToolResponse { .. }
        | Op::ResolveElicitation { .. }) => {
            response_ops::handle(sess, op).await;
            DispatchOutcome::Continue
        }
        op @ (Op::AddToHistory { .. }
        | Op::GetHistoryEntryRequest { .. }
        | Op::ListMcpTools
        | Op::RefreshMcpServers { .. }
        | Op::ReloadUserConfig
        | Op::ListSkills { .. }) => {
            command_ops::handle(sess, config, sub_id, op).await;
            DispatchOutcome::Continue
        }
        op @ (Op::Undo
        | Op::Compact
        | Op::DropMemories
        | Op::UpdateMemories
        | Op::ThreadRollback { .. }
        | Op::SetThreadName { .. }
        | Op::RunUserShellCommand { .. }
        | Op::Review { .. }) => {
            task_ops::handle(sess, config, sub_id, op).await;
            DispatchOutcome::Continue
        }
        Op::Shutdown => {
            if sess.shutdown_from_submission(sub_id).await {
                DispatchOutcome::Exit
            } else {
                DispatchOutcome::Continue
            }
        }
        _ => DispatchOutcome::Continue,
    }
}
