use super::*;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = CloseAgentResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            call_id,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: CloseAgentArgs = parse_arguments(&arguments)?;
        let agent_id = resolve_agent_target(&session, &turn, &args.target).await?;
        let receiver_agent = session
            .services
            .agent_control
            .get_live_agent_metadata(agent_id)
            .await
            .unwrap_or_default();
        if receiver_agent
            .agent_path
            .as_ref()
            .is_some_and(AgentPath::is_root)
        {
            return Err(FunctionCallError::RespondToModel(
                "root is not a spawned agent".to_string(),
            ));
        }
        let collab_events = CollabAgentEventEmitter::new(session.as_ref(), turn.as_ref(), &call_id);
        collab_events.close_begin(agent_id).await;
        let status = match session
            .services
            .agent_control
            .subscribe_status(agent_id)
            .await
        {
            Ok(mut status_rx) => status_rx.borrow_and_update().clone(),
            Err(err) => {
                let status = session.services.agent_control.get_status(agent_id).await;
                collab_events
                    .close_end(CollabCloseEndEventInput {
                        receiver_thread_id: agent_id,
                        receiver_agent_base_name: receiver_agent.agent_base_name.clone(),
                        receiver_agent_title: receiver_agent.agent_title.clone(),
                        receiver_agent_display_name: receiver_agent.agent_display_name.clone(),
                        receiver_agent_role: receiver_agent.agent_role.clone(),
                        status,
                    })
                    .await;
                return Err(collab_agent_error(agent_id, err));
            }
        };
        let result = session
            .services
            .agent_control
            .close_agent(agent_id)
            .await
            .map_err(|err| collab_agent_error(agent_id, err))
            .map(|_| ());
        collab_events
            .close_end(CollabCloseEndEventInput {
                receiver_thread_id: agent_id,
                receiver_agent_base_name: receiver_agent.agent_base_name,
                receiver_agent_title: receiver_agent.agent_title,
                receiver_agent_display_name: receiver_agent.agent_display_name,
                receiver_agent_role: receiver_agent.agent_role,
                status: status.clone(),
            })
            .await;
        result?;

        Ok(CloseAgentResult {
            previous_status: status,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseAgentArgs {
    target: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CloseAgentResult {
    pub(crate) previous_status: AgentStatus,
}

impl ToolOutput for CloseAgentResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "close_agent")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "close_agent")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "close_agent")
    }
}
