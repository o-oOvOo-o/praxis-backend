use super::*;

impl AgentOs {
    pub(crate) async fn open_execution(
        self: &Arc<Self>,
        request: AgentOsExecutionOpenRequest<'_>,
    ) -> PraxisResult<ManagedCommandSpan> {
        let ticket = self
            .request_command_ticket(request.thread_id, request.argv, request.cwd)
            .await?;
        let command_id = match self
            .begin_managed_command(
                &ticket,
                request.command,
                request.argv,
                request.cwd.to_path_buf(),
                request.process_id,
                request.runtime_kind.map(str::to_string),
                request.runtime_owner_id.map(str::to_string),
            )
            .await
        {
            Ok(command_id) => command_id,
            Err(err) => {
                self.revoke_unstarted_ticket(&ticket, err.to_string()).await;
                return Err(err);
            }
        };
        Ok(ManagedCommandSpan::new(Arc::clone(self), command_id))
    }
}
