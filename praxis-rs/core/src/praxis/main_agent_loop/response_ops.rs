use std::sync::Arc;

use praxis_protocol::protocol::Op;

use crate::praxis::Session;

pub(super) async fn handle(sess: &Arc<Session>, op: Op) {
    match op {
        Op::ExecApproval {
            id: approval_id,
            turn_id,
            decision,
        } => {
            sess.apply_exec_approval(approval_id, turn_id, decision)
                .await;
        }
        Op::PatchApproval { id, decision } => {
            sess.apply_patch_approval(id, decision).await;
        }
        Op::UserInputAnswer { id, response } => {
            sess.notify_user_input_response(&id, response).await;
        }
        Op::RequestPermissionsResponse { id, response } => {
            sess.notify_request_permissions_response(&id, response)
                .await;
        }
        Op::DynamicToolResponse { id, response } => {
            sess.notify_dynamic_tool_response(&id, response).await;
        }
        Op::ResolveElicitation {
            server_name,
            request_id,
            decision,
            content,
            meta,
        } => {
            sess.apply_elicitation_response(server_name, request_id, decision, content, meta)
                .await;
        }
        _ => {}
    }
}
