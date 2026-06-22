use crate::agent_os::model::RuntimeCommandStatus;
use crate::agent_os::model::TaskStatus;
use crate::agent_os::model::ThreadRuntimeState;

impl RuntimeCommandStatus {
    pub(in crate::agent_os) fn is_unclaimed(self) -> bool {
        self == Self::Pending
    }

    pub(in crate::agent_os) fn is_live(self) -> bool {
        matches!(self, Self::Pending | Self::Acked | Self::Executing)
    }

    pub(in crate::agent_os) fn assign_task_status(self) -> TaskStatus {
        match self {
            Self::Executing => TaskStatus::Running,
            Self::Completed => TaskStatus::Completed,
            Self::Failed | Self::Expired => TaskStatus::Failed,
            Self::Rejected => TaskStatus::Cancelled,
            Self::Pending | Self::Acked => TaskStatus::Assigned,
        }
    }

    pub(in crate::agent_os) fn assign_thread_state(self) -> ThreadRuntimeState {
        match self {
            Self::Executing => ThreadRuntimeState::Running,
            Self::Completed | Self::Failed | Self::Expired | Self::Rejected => {
                ThreadRuntimeState::Idle
            }
            Self::Pending | Self::Acked => ThreadRuntimeState::Assigned,
        }
    }

    pub(in crate::agent_os) fn clears_assigned_task(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Expired | Self::Rejected
        )
    }

    pub(in crate::agent_os) fn active_selection_rank(self) -> i8 {
        match self {
            Self::Executing => 2,
            Self::Acked => 1,
            Self::Pending => 0,
            Self::Completed | Self::Failed | Self::Expired | Self::Rejected => -1,
        }
    }

    pub(in crate::agent_os) fn is_turn_claimed(self) -> bool {
        matches!(self, Self::Acked | Self::Executing)
    }
}
