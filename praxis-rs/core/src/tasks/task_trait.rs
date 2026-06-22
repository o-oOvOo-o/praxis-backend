use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::state::AgentTaskKind;
use praxis_protocol::user_input::UserInput;

#[async_trait]
pub(crate) trait AgentTask: Send + Sync + 'static {
    fn kind(&self) -> AgentTaskKind;

    fn span_name(&self) -> &'static str;

    async fn run(
        self: Arc<Self>,
        session: Arc<Session>,
        ctx: Arc<TurnContext>,
        input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String>;

    async fn abort(&self, session: Arc<Session>, ctx: Arc<TurnContext>) {
        let _ = (session, ctx);
    }
}

#[cfg(test)]
pub(crate) type AgentTaskContext = Session;
