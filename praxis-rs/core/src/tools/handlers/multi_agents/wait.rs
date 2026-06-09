use super::*;
use crate::agent::status::is_final;
use crate::agent_os::AgentOsRuntime;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::CollabAgentRef;
use std::collections::HashMap;
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
        let target = args
            .target
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let timeout_ms = args.timeout_ms.unwrap_or(DEFAULT_WAIT_TIMEOUT_MS);
        let timeout_ms = match timeout_ms {
            ms if ms <= 0 => {
                return Err(FunctionCallError::RespondToModel(
                    "timeout_ms must be greater than zero".to_owned(),
                ));
            }
            ms => ms.clamp(MIN_WAIT_TIMEOUT_MS, MAX_WAIT_TIMEOUT_MS),
        };
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);

        if let Some(target) = target {
            return handle_target_wait(session, turn, call_id, target, deadline).await;
        }

        handle_global_wait(session, turn, call_id, deadline).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitArgs {
    target: Option<String>,
    timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct WaitAgentResult {
    pub(crate) message: String,
    pub(crate) timed_out: bool,
    pub(crate) source: String,
    pub(crate) agent_os_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_thread_id: Option<ThreadId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_agent_base_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_agent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_agent_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_status: Option<AgentStatus>,
    pub(crate) next_action: String,
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

#[derive(Debug, PartialEq, Eq)]
struct TargetWaitOutcome {
    status: AgentStatus,
    timed_out: bool,
    already_final: bool,
    agent_os_sequence: u64,
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
            target: None,
            target_thread_id: None,
            target_agent_base_name: None,
            target_agent_title: None,
            target_agent_display_name: None,
            target_status: None,
            next_action:
                "Use list_agents for current worker state; use targeted wait_agent for a specific worker result."
                    .to_string(),
        }
    }

    fn from_target_outcome(
        target: &str,
        target_thread_id: ThreadId,
        target_agent_base_name: Option<String>,
        target_agent_title: Option<String>,
        target_agent_display_name: Option<String>,
        outcome: TargetWaitOutcome,
    ) -> Self {
        let timed_out = outcome.timed_out;
        let already_final = outcome.already_final;
        let agent_os_sequence = outcome.agent_os_sequence;
        let status = outcome.status;
        let message = if timed_out {
            "Wait timed out before target agent reached a final status.".to_string()
        } else if already_final {
            "Wait completed because target agent had already reached a final status.".to_string()
        } else {
            "Wait completed because target agent reached a final status.".to_string()
        };
        Self {
            message,
            timed_out,
            source: if timed_out {
                "timeout"
            } else {
                "target_status"
            }
            .to_string(),
            agent_os_sequence: Some(agent_os_sequence),
            target: Some(target.to_string()),
            target_thread_id: Some(target_thread_id),
            target_agent_base_name,
            target_agent_title,
            target_agent_display_name,
            target_status: Some(status),
            next_action: if timed_out {
                "Target is not final yet. Wait again with this target only if the result is still on the critical path, or assign_task with interrupt=true to redirect it."
            } else {
                "Inspect the worker output and marker; use assign_task for another turn or close_agent when it is no longer needed."
            }
            .to_string(),
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

async fn handle_target_wait(
    session: Arc<crate::praxis::Session>,
    turn: Arc<crate::praxis::TurnContext>,
    call_id: String,
    target: &str,
    deadline: Instant,
) -> Result<WaitAgentResult, FunctionCallError> {
    let receiver_thread_id = resolve_agent_target(&session, &turn, target).await?;
    let receiver_agent = session
        .services
        .agent_control
        .get_live_agent_metadata(receiver_thread_id)
        .await
        .unwrap_or_default();
    let receiver_agents = vec![CollabAgentRef {
        thread_id: receiver_thread_id,
        agent_base_name: receiver_agent.agent_base_name.clone(),
        agent_title: receiver_agent.agent_title.clone(),
        agent_display_name: receiver_agent.agent_display_name.clone(),
        agent_role: receiver_agent.agent_role.clone(),
    }];
    session
        .send_event(
            &turn,
            CollabWaitingBeginEvent {
                sender_thread_id: session.conversation_id,
                receiver_thread_ids: vec![receiver_thread_id],
                receiver_agents: receiver_agents.clone(),
                call_id: call_id.clone(),
            }
            .into(),
        )
        .await;

    let outcome = wait_for_target_status(&session, receiver_thread_id, deadline).await?;
    let result = WaitAgentResult::from_target_outcome(
        target,
        receiver_thread_id,
        receiver_agent.agent_base_name.clone(),
        receiver_agent.agent_title.clone(),
        receiver_agent.agent_display_name.clone(),
        outcome,
    );
    let mut statuses = HashMap::new();
    statuses.insert(
        receiver_thread_id,
        result.target_status.clone().unwrap_or_default(),
    );
    session
        .send_event(
            &turn,
            CollabWaitingEndEvent {
                sender_thread_id: session.conversation_id,
                call_id,
                agent_statuses: build_wait_agent_statuses(&statuses, &receiver_agents),
                statuses,
            }
            .into(),
        )
        .await;
    Ok(result)
}

async fn handle_global_wait(
    session: Arc<crate::praxis::Session>,
    turn: Arc<crate::praxis::TurnContext>,
    call_id: String,
    deadline: Instant,
) -> Result<WaitAgentResult, FunctionCallError> {
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
                statuses: HashMap::new(),
            }
            .into(),
        )
        .await;

    Ok(result)
}

async fn wait_for_target_status(
    session: &Arc<crate::praxis::Session>,
    target_thread_id: ThreadId,
    deadline: Instant,
) -> Result<TargetWaitOutcome, FunctionCallError> {
    let mut status_rx = session
        .services
        .agent_control
        .subscribe_status(target_thread_id)
        .await
        .map_err(|err| collab_agent_error(target_thread_id, err))?;
    let current = status_rx.borrow().clone();
    if is_final(&current) {
        return Ok(TargetWaitOutcome {
            status: current,
            timed_out: false,
            already_final: true,
            agent_os_sequence: session.services.agent_os.change_sequence(),
        });
    }

    loop {
        if Instant::now() >= deadline {
            return Ok(TargetWaitOutcome {
                status: status_rx.borrow().clone(),
                timed_out: true,
                already_final: false,
                agent_os_sequence: session.services.agent_os.change_sequence(),
            });
        }

        select! {
            status_changed = status_rx.changed() => {
                if status_changed.is_err() {
                    return Ok(TargetWaitOutcome {
                        status: AgentStatus::NotFound,
                        timed_out: false,
                        already_final: false,
                        agent_os_sequence: session.services.agent_os.change_sequence(),
                    });
                }
                let status = status_rx.borrow().clone();
                if is_final(&status) {
                    return Ok(TargetWaitOutcome {
                        status,
                        timed_out: false,
                        already_final: false,
                        agent_os_sequence: session.services.agent_os.change_sequence(),
                    });
                }
            }
            _ = sleep_until(deadline) => {
                return Ok(TargetWaitOutcome {
                    status: status_rx.borrow().clone(),
                    timed_out: true,
                    already_final: false,
                    agent_os_sequence: session.services.agent_os.change_sequence(),
                });
            }
        }
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
