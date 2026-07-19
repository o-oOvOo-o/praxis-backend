use std::sync::Arc;

use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use praxis_utils_readiness::Token;
use tokio_util::sync::CancellationToken;

use super::capture::run_workspace_checkpoint_capture;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;
use crate::tasks::AgentTask;

struct WorkspaceCheckpointTask {
    token: Token,
}

impl WorkspaceCheckpointTask {
    fn new(token: Token) -> Self {
        Self { token }
    }
}

impl Session {
    pub(crate) async fn run_workspace_checkpoint_task(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        token: Token,
        cancellation_token: CancellationToken,
    ) {
        Arc::new(WorkspaceCheckpointTask::new(token))
            .run(
                Arc::clone(self),
                turn_context,
                Vec::new(),
                cancellation_token,
            )
            .await;
    }
}

#[async_trait]
impl AgentTask for WorkspaceCheckpointTask {
    fn kind(&self) -> AgentTaskKind {
        AgentTaskKind::WorkspaceCheckpoint
    }

    fn span_name(&self) -> &'static str {
        "agent_task.workspace_checkpoint"
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        _input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        tokio::task::spawn(run_workspace_checkpoint_capture(
            session,
            ctx,
            self.token,
            cancellation_token,
        ));
        None
    }
}
