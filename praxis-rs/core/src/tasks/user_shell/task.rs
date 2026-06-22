use std::sync::Arc;

use async_trait::async_trait;
use praxis_protocol::user_input::UserInput;
use tokio_util::sync::CancellationToken;

use super::execution::execute_user_shell_command;
use super::types::UserShellCommandMode;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;
use crate::tasks::AgentTask;

#[derive(Clone)]
struct UserShellCommandTask {
    command: String,
}

impl UserShellCommandTask {
    fn new(command: String) -> Self {
        Self { command }
    }
}

impl Session {
    pub(crate) async fn run_user_shell_command_task(
        self: &Arc<Self>,
        sub_id: String,
        command: String,
    ) {
        if let Some((turn_context, cancellation_token)) =
            self.active_turn_context_and_cancellation_token().await
        {
            let session = Arc::clone(self);
            tokio::spawn(async move {
                execute_user_shell_command(
                    session,
                    turn_context,
                    command,
                    cancellation_token,
                    UserShellCommandMode::ActiveTurnAuxiliary,
                )
                .await;
            });
            return;
        }

        let turn_context = self.new_default_turn_with_sub_id(sub_id).await;
        self.spawn_task(
            Arc::clone(&turn_context),
            Vec::new(),
            UserShellCommandTask::new(command),
        )
        .await;
    }
}

#[async_trait]
impl AgentTask for UserShellCommandTask {
    fn kind(&self) -> AgentTaskKind {
        AgentTaskKind::UserShell
    }

    fn span_name(&self) -> &'static str {
        "agent_task.user_shell"
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        turn_context: Arc<TurnContext>,
        _input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        execute_user_shell_command(
            session,
            turn_context,
            self.command.clone(),
            cancellation_token,
            UserShellCommandMode::StandaloneTurn,
        )
        .await;
        None
    }
}
