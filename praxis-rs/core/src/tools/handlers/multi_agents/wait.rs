use super::*;
use crate::agent_os::AgentOsRuntime;
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::time::Instant;
use tokio::time::sleep_until;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = WaitAgentResult;

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
        let args: WaitArgs = parse_arguments(&arguments)?;
        let timeout_ms = args.timeout_ms.unwrap_or(DEFAULT_WAIT_TIMEOUT_MS);
        let timeout_ms = match timeout_ms {
            ms if ms <= 0 => {
                return Err(FunctionCallError::RespondToModel(
                    "timeout_ms must be greater than zero".to_owned(),
                ));
            }
            ms => ms.clamp(MIN_WAIT_TIMEOUT_MS, MAX_WAIT_TIMEOUT_MS),
        };

        let mut mailbox_seq_rx = session.subscribe_mailbox_seq();
        let agent_os = Arc::clone(&session.services.agent_os);
        let before_agent_os_seq = agent_os.change_sequence();

        session
            .send_event(
                &turn,
                CollabWaitingBeginEvent {
                    sender_thread_id: session.conversation_id,
                    receiver_thread_ids: Vec::new(),
                    receiver_agents: Vec::new(),
                    call_id: call_id.clone(),
                }
                .into(),
            )
            .await;

        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
        let outcome = wait_for_mailbox_or_agent_os_change(
            &mut mailbox_seq_rx,
            agent_os,
            before_agent_os_seq,
            deadline,
        )
        .await;
        let result = WaitAgentResult::from_outcome(outcome);

        session
            .send_event(
                &turn,
                CollabWaitingEndEvent {
                    sender_thread_id: session.conversation_id,
                    call_id,
                    agent_statuses: Vec::new(),
                    statuses: std::collections::HashMap::new(),
                }
                .into(),
            )
            .await;

        Ok(result)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitArgs {
    timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct WaitAgentResult {
    pub(crate) message: String,
    pub(crate) timed_out: bool,
    pub(crate) source: String,
    pub(crate) agent_os_sequence: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
struct WaitOutcome {
    source: WaitSource,
    agent_os_sequence: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
enum WaitSource {
    Mailbox,
    AgentOs,
    Timeout,
}

impl WaitAgentResult {
    fn from_outcome(outcome: WaitOutcome) -> Self {
        let (message, timed_out, source) = match outcome.source {
            WaitSource::Mailbox => (
                "Wait completed because mailbox input arrived.".to_string(),
                false,
                "mailbox".to_string(),
            ),
            WaitSource::AgentOs => (
                "Wait completed because AgentOS runtime state changed.".to_string(),
                false,
                "agent_os".to_string(),
            ),
            WaitSource::Timeout => ("Wait timed out.".to_string(), true, "timeout".to_string()),
        };
        Self {
            message,
            timed_out,
            source,
            agent_os_sequence: outcome.agent_os_sequence,
        }
    }
}

impl ToolOutput for WaitAgentResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "wait_agent")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, /*success*/ None, "wait_agent")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "wait_agent")
    }
}

async fn wait_for_mailbox_or_agent_os_change(
    mailbox_seq_rx: &mut tokio::sync::watch::Receiver<u64>,
    agent_os: Arc<AgentOsRuntime>,
    before_agent_os_seq: u64,
    deadline: Instant,
) -> WaitOutcome {
    let mut agent_os_rx = agent_os.subscribe_changes();
    loop {
        let current_seq = agent_os.change_sequence();
        if current_seq > before_agent_os_seq {
            return WaitOutcome {
                source: WaitSource::AgentOs,
                agent_os_sequence: Some(current_seq),
            };
        }
        if Instant::now() >= deadline {
            return WaitOutcome {
                source: WaitSource::Timeout,
                agent_os_sequence: Some(current_seq),
            };
        }
        select! {
            mailbox = mailbox_seq_rx.changed() => {
                if mailbox.is_ok() {
                    return WaitOutcome {
                        source: WaitSource::Mailbox,
                        agent_os_sequence: Some(agent_os.change_sequence()),
                    };
                }
                return WaitOutcome {
                    source: WaitSource::Timeout,
                    agent_os_sequence: Some(agent_os.change_sequence()),
                };
            }
            agent_os_changed = agent_os_rx.changed() => {
                if agent_os_changed.is_err() {
                    return WaitOutcome {
                        source: WaitSource::Timeout,
                        agent_os_sequence: Some(agent_os.change_sequence()),
                    };
                }
                let current_seq = *agent_os_rx.borrow();
                if current_seq > before_agent_os_seq {
                    return WaitOutcome {
                        source: WaitSource::AgentOs,
                        agent_os_sequence: Some(current_seq),
                    };
                }
            }
            _ = sleep_until(deadline) => {
                return WaitOutcome {
                    source: WaitSource::Timeout,
                    agent_os_sequence: Some(agent_os.change_sequence()),
                };
            }
        }
    }
}
