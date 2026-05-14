use std::collections::VecDeque;

use praxis_protocol::approvals::ElicitationRequestEvent;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::ExecApprovalRequestEvent;
use praxis_protocol::protocol::ExecCommandBeginEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::McpToolCallBeginEvent;
use praxis_protocol::protocol::McpToolCallEndEvent;
use praxis_protocol::protocol::PatchApplyEndEvent;
use praxis_protocol::request_permissions::RequestPermissionsEvent;
use praxis_protocol::request_user_input::RequestUserInputEvent;

use super::ChatWidget;

#[derive(Debug)]
pub(crate) enum QueuedInterrupt {
    ExecApproval(ExecApprovalRequestEvent),
    ApplyPatchApproval(ApplyPatchApprovalRequestEvent),
    Elicitation(ElicitationRequestEvent),
    RequestPermissions(RequestPermissionsEvent),
    RequestUserInput(RequestUserInputEvent),
    ExecBegin(ExecCommandBeginEvent),
    ExecEnd(ExecCommandEndEvent),
    McpBegin(McpToolCallBeginEvent),
    McpEnd(McpToolCallEndEvent),
    PatchEnd(PatchApplyEndEvent),
}

#[derive(Default)]
pub(crate) struct InterruptManager {
    queue: VecDeque<QueuedInterrupt>,
}

impl InterruptManager {
    pub(crate) fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub(crate) fn push_exec_approval(&mut self, ev: ExecApprovalRequestEvent) {
        self.queue.push_back(QueuedInterrupt::ExecApproval(ev));
    }

    pub(crate) fn push_apply_patch_approval(&mut self, ev: ApplyPatchApprovalRequestEvent) {
        self.queue
            .push_back(QueuedInterrupt::ApplyPatchApproval(ev));
    }

    pub(crate) fn push_elicitation(&mut self, ev: ElicitationRequestEvent) {
        self.queue.push_back(QueuedInterrupt::Elicitation(ev));
    }

    pub(crate) fn push_request_permissions(&mut self, ev: RequestPermissionsEvent) {
        self.queue
            .push_back(QueuedInterrupt::RequestPermissions(ev));
    }

    pub(crate) fn push_user_input(&mut self, ev: RequestUserInputEvent) {
        self.queue.push_back(QueuedInterrupt::RequestUserInput(ev));
    }

    pub(crate) fn push_exec_begin(&mut self, ev: ExecCommandBeginEvent) {
        self.queue.push_back(QueuedInterrupt::ExecBegin(ev));
    }

    pub(crate) fn push_exec_end(&mut self, ev: ExecCommandEndEvent) {
        self.queue.push_back(QueuedInterrupt::ExecEnd(ev));
    }

    pub(crate) fn push_mcp_begin(&mut self, ev: McpToolCallBeginEvent) {
        self.queue.push_back(QueuedInterrupt::McpBegin(ev));
    }

    pub(crate) fn push_mcp_end(&mut self, ev: McpToolCallEndEvent) {
        self.queue.push_back(QueuedInterrupt::McpEnd(ev));
    }

    pub(crate) fn push_patch_end(&mut self, ev: PatchApplyEndEvent) {
        self.queue.push_back(QueuedInterrupt::PatchEnd(ev));
    }

    pub(crate) fn flush_all(&mut self, chat: &mut ChatWidget) {
        while let Some(q) = self.queue.pop_front() {
            match q {
                QueuedInterrupt::ExecApproval(ev) => chat.handle_exec_approval_now(ev),
                QueuedInterrupt::ApplyPatchApproval(ev) => chat.handle_apply_patch_approval_now(ev),
                QueuedInterrupt::Elicitation(ev) => chat.handle_elicitation_request_now(ev),
                QueuedInterrupt::RequestPermissions(ev) => chat.handle_request_permissions_now(ev),
                QueuedInterrupt::RequestUserInput(ev) => chat.handle_request_user_input_now(ev),
                QueuedInterrupt::ExecBegin(ev) => chat.handle_exec_begin_now(ev),
                QueuedInterrupt::ExecEnd(ev) => chat.handle_exec_end_now(ev),
                QueuedInterrupt::McpBegin(ev) => chat.handle_mcp_begin_now(ev),
                QueuedInterrupt::McpEnd(ev) => chat.handle_mcp_end_now(ev),
                QueuedInterrupt::PatchEnd(ev) => chat.handle_patch_apply_end_now(ev),
            }
        }
    }
}
