use praxis_features::Feature;
use praxis_otel::current_span_w3c_trace_context;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::Submission;
use praxis_protocol::protocol::W3cTraceContext;
use praxis_protocol::user_input::UserInput;
use praxis_rollout::state_db;
use uuid::Uuid;

use crate::agent::AgentStatus;
use crate::config::ConstraintResult;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::praxis_thread::ThreadConfigSnapshot;

use super::super::Praxis;
use super::super::SessionSettingsUpdate;
use super::super::SteerInputError;

impl Praxis {
    /// Submit the `op` wrapped in a `Submission` with a unique ID.
    pub async fn submit(&self, op: Op) -> PraxisResult<String> {
        self.submit_with_trace(op, /*trace*/ None).await
    }

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> PraxisResult<String> {
        let id = Uuid::now_v7().to_string();
        let sub = Submission {
            id: id.clone(),
            op,
            trace,
        };
        self.submit_with_id(sub).await?;
        Ok(id)
    }

    /// Use sparingly: prefer `submit()` so Praxis is responsible for generating
    /// unique IDs for each submission.
    pub async fn submit_with_id(&self, mut sub: Submission) -> PraxisResult<()> {
        if sub.trace.is_none() {
            sub.trace = current_span_w3c_trace_context();
        }
        self.tx_sub
            .send(sub)
            .await
            .map_err(|_| PraxisErr::InternalAgentDied)?;
        Ok(())
    }

    pub async fn shutdown_and_wait(&self) -> PraxisResult<()> {
        let session_loop_termination = self.session_loop_termination.clone();
        match self.submit(Op::Shutdown).await {
            Ok(_) => {}
            Err(PraxisErr::InternalAgentDied) => {}
            Err(err) => return Err(err),
        }
        session_loop_termination.await;
        Ok(())
    }

    pub async fn next_event(&self) -> PraxisResult<Event> {
        let event = self
            .rx_event
            .recv()
            .await
            .map_err(|_| PraxisErr::InternalAgentDied)?;
        Ok(event)
    }

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
    ) -> Result<String, SteerInputError> {
        self.session.steer_input(input, expected_turn_id).await
    }

    pub(crate) async fn set_app_gateway_client_name(
        &self,
        app_gateway_client_name: Option<String>,
    ) -> ConstraintResult<()> {
        self.session
            .update_settings(SessionSettingsUpdate {
                app_gateway_client_name,
                ..Default::default()
            })
            .await
    }

    pub(crate) async fn agent_status(&self) -> AgentStatus {
        self.agent_status.borrow().clone()
    }

    pub(crate) async fn thread_config_snapshot(&self) -> ThreadConfigSnapshot {
        let state = self.session.state.lock().await;
        state.session_configuration.thread_config_snapshot()
    }

    pub(crate) fn state_db(&self) -> Option<state_db::StateDbHandle> {
        self.session.state_db()
    }

    pub(crate) fn enabled(&self, feature: Feature) -> bool {
        self.session.enabled(feature)
    }
}
