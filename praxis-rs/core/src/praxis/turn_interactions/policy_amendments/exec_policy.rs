use praxis_protocol::approvals::ExecPolicyAmendment;

use crate::exec_policy::ExecPolicyUpdateError;
use crate::praxis::Session;

impl Session {
    /// Adds an execpolicy amendment to both the in-memory and on-disk policies.
    pub(crate) async fn persist_execpolicy_amendment(
        &self,
        amendment: &ExecPolicyAmendment,
    ) -> Result<(), ExecPolicyUpdateError> {
        let praxis_home = self
            .state
            .lock()
            .await
            .session_configuration
            .praxis_home()
            .clone();

        self.services
            .exec_policy
            .append_amendment_and_update(&praxis_home, amendment)
            .await?;

        Ok(())
    }
}
