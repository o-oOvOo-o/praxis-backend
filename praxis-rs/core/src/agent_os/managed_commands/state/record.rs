use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use praxis_protocol::ThreadId;

use crate::agent_os::records::ActiveCoordinatorLease;
use crate::agent_os::records::RuntimeCommandRecord;
use crate::agent_os::records::RuntimeCommandStatus;
use crate::agent_os::records::RuntimeCommandType;

use super::activity::RuntimeCommandActivity;

impl RuntimeCommandRecord {
    pub(in crate::agent_os) fn apply_activity(
        &mut self,
        activity: RuntimeCommandActivity,
        current_task_id: Option<&str>,
        now: DateTime<Utc>,
        ttl: Duration,
    ) -> bool {
        if self.expires_at <= now {
            if self.status.is_live() {
                self.status = RuntimeCommandStatus::Expired;
                self.updated_at = now;
                return true;
            }
            return false;
        }

        let mut changed = false;
        if self.status.is_live() {
            self.expires_at = now + ttl;
            self.updated_at = now;
            changed = true;
        }
        match (activity, self.status, self.command_type) {
            (_, RuntimeCommandStatus::Pending, _) => {
                self.status = RuntimeCommandStatus::Acked;
                changed = true;
            }
            (
                RuntimeCommandActivity::WorkerStartedCommand,
                RuntimeCommandStatus::Acked,
                RuntimeCommandType::AssignTask,
            ) if self.task_id.as_deref() == current_task_id => {
                self.status = RuntimeCommandStatus::Executing;
                changed = true;
            }
            _ => {}
        }
        changed
    }

    pub(in crate::agent_os) fn matches_coordinator(
        &self,
        active: Option<&ActiveCoordinatorLease>,
    ) -> bool {
        active.is_some_and(|active| {
            self.coordinator_epoch == active.epoch && self.fencing_token == active.fencing_token
        })
    }

    pub(in crate::agent_os) fn claim_status(
        &self,
        active: Option<&ActiveCoordinatorLease>,
        now: DateTime<Utc>,
    ) -> RuntimeCommandStatus {
        if self.expires_at <= now {
            RuntimeCommandStatus::Expired
        } else if !self.matches_coordinator(active) {
            RuntimeCommandStatus::Rejected
        } else if self.command_type == RuntimeCommandType::AssignTask {
            RuntimeCommandStatus::Executing
        } else {
            RuntimeCommandStatus::Acked
        }
    }

    pub(in crate::agent_os) fn poll_status(
        &self,
        active: Option<&ActiveCoordinatorLease>,
        now: DateTime<Utc>,
        auto_ack: bool,
    ) -> RuntimeCommandStatus {
        if self.expires_at <= now {
            RuntimeCommandStatus::Expired
        } else if !self.matches_coordinator(active) {
            RuntimeCommandStatus::Rejected
        } else if auto_ack && self.status == RuntimeCommandStatus::Pending {
            RuntimeCommandStatus::Acked
        } else {
            self.status
        }
    }

    pub(in crate::agent_os) fn reported_status(
        &self,
        actor_thread_id: ThreadId,
        status: RuntimeCommandStatus,
        active: Option<&ActiveCoordinatorLease>,
        now: DateTime<Utc>,
    ) -> RuntimeCommandStatus {
        let receiver_terminal_report = actor_thread_id == self.to_thread_id
            && matches!(
                status,
                RuntimeCommandStatus::Completed | RuntimeCommandStatus::Failed
            )
            && matches!(
                self.status,
                RuntimeCommandStatus::Acked | RuntimeCommandStatus::Executing
            );
        if receiver_terminal_report
            || matches!(
                status,
                RuntimeCommandStatus::Failed | RuntimeCommandStatus::Rejected
            )
        {
            status
        } else if self.expires_at <= now {
            RuntimeCommandStatus::Expired
        } else if !self.matches_coordinator(active) {
            RuntimeCommandStatus::Rejected
        } else {
            status
        }
    }
}
