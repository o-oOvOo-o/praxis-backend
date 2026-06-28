use super::*;
use crate::app_gateway_core_conversions::app_gateway_web_search_action_to_core;
use crate::app_gateway_session::token_usage_info_from_app_gateway;
use crate::exec_command::split_command_string;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::items::AgentMessageContent;
use praxis_protocol::items::AgentMessageItem;
use praxis_protocol::items::ContextCompactionItem;
use praxis_protocol::items::ImageGenerationItem;
use praxis_protocol::items::PlanItem;
use praxis_protocol::items::ReasoningItem;
use praxis_protocol::items::TurnItem;
use praxis_protocol::items::UserMessageItem;
use praxis_protocol::items::WebSearchItem;
use praxis_protocol::protocol::AgentMessageDeltaEvent;
use praxis_protocol::protocol::AgentReasoningDeltaEvent;
use praxis_protocol::protocol::AgentReasoningRawContentDeltaEvent;
use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecCommandBeginEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::ExecCommandOutputDeltaEvent;
use praxis_protocol::protocol::ExecCommandStatus;
use praxis_protocol::protocol::ExecOutputStream;
use praxis_protocol::protocol::ItemCompletedEvent;
use praxis_protocol::protocol::ItemStartedEvent;
use praxis_protocol::protocol::PlanDeltaEvent;
use praxis_protocol::protocol::RealtimeConversationClosedEvent;
use praxis_protocol::protocol::RealtimeConversationRealtimeEvent;
use praxis_protocol::protocol::RealtimeConversationStartedEvent;
use praxis_protocol::protocol::RealtimeEvent;
use praxis_protocol::protocol::ThreadNameUpdatedEvent;
use praxis_protocol::protocol::TokenCountEvent;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnAbortedEvent;
use praxis_protocol::protocol::TurnCompleteEvent;
use praxis_protocol::protocol::TurnStartedEvent;
use std::time::Duration;

#[cfg(test)]
/// Convert a `Thread` snapshot into a flat sequence of protocol `Event`s
/// suitable for replaying into the TUI event store.
///
/// Each turn is expanded into `TurnStarted`, zero or more `ItemCompleted`,
/// and a terminal event that matches the turn's `TurnStatus`. Returns an
/// empty vec (with a warning log) if the thread ID is not a valid UUID.
pub(super) fn thread_snapshot_events(
    thread: &Thread,
    show_raw_agent_reasoning: bool,
) -> Vec<Event> {
    let Ok(thread_id) = ThreadId::from_string(&thread.id) else {
        tracing::warn!(
            thread_id = %thread.id,
            "ignoring app-gateway thread snapshot with invalid thread id"
        );
        return Vec::new();
    };

    thread
        .turns
        .iter()
        .flat_map(|turn| turn_snapshot_events(thread_id, turn, show_raw_agent_reasoning))
        .collect()
}

#[cfg(test)]
fn server_notification_thread_events(
    notification: ServerNotification,
) -> Option<(ThreadId, Vec<Event>)> {
    let thread_id =
        server_notification_thread_id(&notification).and_then(parse_app_gateway_thread_id)?;
    match notification {
        ServerNotification::ThreadTokenUsageUpdated(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::TokenCount(TokenCountEvent {
                    info: Some(token_usage_info_from_app_gateway(notification.token_usage)),
                    rate_limits: None,
                }),
            }],
        )),
        ServerNotification::Error(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::Error(ErrorEvent {
                    message: notification.error.message,
                    praxis_error_info: notification
                        .error
                        .praxis_error_info
                        .and_then(app_gateway_praxis_error_info_to_core),
                }),
            }],
        )),
        ServerNotification::ThreadNameUpdated(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::ThreadNameUpdated(ThreadNameUpdatedEvent {
                    thread_id,
                    thread_name: notification.thread_name,
                }),
            }],
        )),
        ServerNotification::TurnStarted(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::TurnStarted(TurnStartedEvent {
                    turn_id: notification.turn.id,
                    model_context_window: notification.model_context_window,
                    collaboration_mode_kind: ModeKind::default(),
                }),
            }],
        )),
        ServerNotification::TurnCompleted(notification) => {
            let mut events = Vec::new();
            append_terminal_turn_events(
                &mut events,
                &notification.turn,
                /*include_failed_error*/ false,
            );
            Some((thread_id, events))
        }
        ServerNotification::ItemStarted(notification) => Some((
            thread_id,
            command_execution_started_event(&notification.turn_id, &notification.item).or_else(
                || {
                    Some(vec![Event {
                        id: String::new(),
                        msg: EventMsg::ItemStarted(ItemStartedEvent {
                            thread_id,
                            turn_id: notification.turn_id.clone(),
                            item: thread_item_to_core(&notification.item)?,
                        }),
                    }])
                },
            )?,
        )),
        ServerNotification::ItemCompleted(notification) => Some((
            thread_id,
            command_execution_completed_event(&notification.turn_id, &notification.item).or_else(
                || {
                    Some(vec![Event {
                        id: String::new(),
                        msg: EventMsg::ItemCompleted(ItemCompletedEvent {
                            thread_id,
                            turn_id: notification.turn_id.clone(),
                            item: thread_item_to_core(&notification.item)?,
                        }),
                    }])
                },
            )?,
        )),
        ServerNotification::CommandExecutionOutputDelta(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::ExecCommandOutputDelta(ExecCommandOutputDeltaEvent {
                    call_id: notification.item_id,
                    stream: ExecOutputStream::Stdout,
                    chunk: notification.delta.into_bytes(),
                }),
            }],
        )),
        ServerNotification::AgentMessageDelta(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
                    delta: notification.delta,
                }),
            }],
        )),
        ServerNotification::PlanDelta(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::PlanDelta(PlanDeltaEvent {
                    thread_id: notification.thread_id,
                    turn_id: notification.turn_id,
                    item_id: notification.item_id,
                    delta: notification.delta,
                }),
            }],
        )),
        ServerNotification::ReasoningSummaryTextDelta(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
                    delta: notification.delta,
                }),
            }],
        )),
        ServerNotification::ReasoningTextDelta(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::AgentReasoningRawContentDelta(AgentReasoningRawContentDeltaEvent {
                    delta: notification.delta,
                }),
            }],
        )),
        ServerNotification::ThreadRealtimeStarted(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::RealtimeConversationStarted(RealtimeConversationStartedEvent {
                    session_id: notification.session_id,
                    version: notification.version,
                }),
            }],
        )),
        ServerNotification::ThreadRealtimeItemAdded(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
                    payload: RealtimeEvent::ConversationItemAdded(notification.item),
                }),
            }],
        )),
        ServerNotification::ThreadRealtimeOutputAudioDelta(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
                    payload: RealtimeEvent::AudioOut(notification.audio.into()),
                }),
            }],
        )),
        ServerNotification::ThreadRealtimeError(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
                    payload: RealtimeEvent::Error(notification.message),
                }),
            }],
        )),
        ServerNotification::ThreadRealtimeClosed(notification) => Some((
            thread_id,
            vec![Event {
                id: String::new(),
                msg: EventMsg::RealtimeConversationClosed(RealtimeConversationClosedEvent {
                    reason: notification.reason,
                }),
            }],
        )),
        _ => None,
    }
}

/// Expand a single `Turn` into the event sequence the TUI would have
/// observed if it had been connected for the turn's entire lifetime.
///
/// Snapshot replay uses committed-item semantics for all transcript items.
#[cfg(test)]
fn turn_snapshot_events(
    thread_id: ThreadId,
    turn: &Turn,
    _show_raw_agent_reasoning: bool,
) -> Vec<Event> {
    let mut events = vec![Event {
        id: String::new(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: turn.id.clone(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::default(),
        }),
    }];

    for item in &turn.items {
        if let Some(command_events) = command_execution_snapshot_events(&turn.id, item) {
            events.extend(command_events);
            continue;
        }

        let Some(item) = thread_item_to_core(item) else {
            continue;
        };
        match item {
            TurnItem::HookPrompt(_) => {}
            item => {
                events.push(Event {
                    id: String::new(),
                    msg: EventMsg::ItemCompleted(ItemCompletedEvent {
                        thread_id,
                        turn_id: turn.id.clone(),
                        item,
                    }),
                });
            }
        }
    }

    append_terminal_turn_events(&mut events, turn, /*include_failed_error*/ true);

    events
}

/// Append the terminal event(s) for a turn based on its `TurnStatus`.
///
/// This function is shared between the live notification bridge
/// (`TurnCompleted` handling) and the snapshot replay path so that both
/// produce identical `EventMsg` sequences for the same turn status.
///
/// - `Completed` → `TurnComplete`
/// - `Interrupted` → `TurnAborted { reason: Interrupted }`
/// - `Failed` → `Error` (if present) then `TurnComplete`
/// - `InProgress` → no events (the turn is still running)
#[cfg(test)]
fn append_terminal_turn_events(events: &mut Vec<Event>, turn: &Turn, include_failed_error: bool) {
    match turn.status {
        TurnStatus::Completed => events.push(Event {
            id: String::new(),
            msg: EventMsg::TurnComplete(TurnCompleteEvent {
                turn_id: turn.id.clone(),
                last_agent_message: None,
            }),
        }),
        TurnStatus::Interrupted => events.push(Event {
            id: String::new(),
            msg: EventMsg::TurnAborted(TurnAbortedEvent {
                turn_id: Some(turn.id.clone()),
                reason: TurnAbortReason::Interrupted,
            }),
        }),
        TurnStatus::Failed => {
            if include_failed_error && let Some(error) = &turn.error {
                events.push(Event {
                    id: String::new(),
                    msg: EventMsg::Error(ErrorEvent {
                        message: error.message.clone(),
                        praxis_error_info: error
                            .praxis_error_info
                            .clone()
                            .and_then(app_gateway_praxis_error_info_to_core),
                    }),
                });
            }
            events.push(Event {
                id: String::new(),
                msg: EventMsg::TurnComplete(TurnCompleteEvent {
                    turn_id: turn.id.clone(),
                    last_agent_message: None,
                }),
            });
        }
        TurnStatus::InProgress => {
            // Preserve unfinished turns during snapshot replay without emitting completion events.
        }
    }
}

#[cfg(test)]
fn thread_item_to_core(item: &ThreadItem) -> Option<TurnItem> {
    match item {
        ThreadItem::UserMessage { id, content } => Some(TurnItem::UserMessage(UserMessageItem {
            id: id.clone(),
            content: content
                .iter()
                .cloned()
                .map(praxis_app_gateway_protocol::UserInput::into_core)
                .collect(),
        })),
        ThreadItem::AgentMessage {
            id,
            text,
            phase,
            memory_citation,
        } => Some(TurnItem::AgentMessage(AgentMessageItem {
            id: id.clone(),
            content: vec![AgentMessageContent::Text { text: text.clone() }],
            phase: phase.clone(),
            memory_citation: memory_citation.clone().map(|citation| {
                praxis_protocol::memory_citation::MemoryCitation {
                    entries: citation
                        .entries
                        .into_iter()
                        .map(
                            |entry| praxis_protocol::memory_citation::MemoryCitationEntry {
                                path: entry.path,
                                line_start: entry.line_start,
                                line_end: entry.line_end,
                                note: entry.note,
                            },
                        )
                        .collect(),
                    rollout_ids: citation.thread_ids,
                }
            }),
        })),
        ThreadItem::Plan { id, text } => Some(TurnItem::Plan(PlanItem {
            id: id.clone(),
            text: text.clone(),
        })),
        ThreadItem::Reasoning {
            id,
            summary,
            content,
        } => Some(TurnItem::Reasoning(ReasoningItem {
            id: id.clone(),
            summary_text: summary.clone(),
            raw_content: content.clone(),
        })),
        ThreadItem::WebSearch { id, query, action } => Some(TurnItem::WebSearch(WebSearchItem {
            id: id.clone(),
            query: query.clone(),
            action: app_gateway_web_search_action_to_core(action.clone()?),
        })),
        ThreadItem::ImageGeneration {
            id,
            status,
            revised_prompt,
            result,
            saved_path,
        } => Some(TurnItem::ImageGeneration(ImageGenerationItem {
            id: id.clone(),
            status: status.clone(),
            revised_prompt: revised_prompt.clone(),
            result: result.clone(),
            saved_path: saved_path.clone(),
        })),
        ThreadItem::ContextCompaction { id } => {
            Some(TurnItem::ContextCompaction(ContextCompactionItem {
                id: id.clone(),
            }))
        }
        ThreadItem::CommandExecution { .. }
        | ThreadItem::FileChange { .. }
        | ThreadItem::McpToolCall { .. }
        | ThreadItem::DynamicToolCall { .. }
        | ThreadItem::CollabAgentToolCall { .. }
        | ThreadItem::HookPrompt { .. }
        | ThreadItem::ImageView { .. }
        | ThreadItem::EnteredReviewMode { .. }
        | ThreadItem::ExitedReviewMode { .. } => {
            tracing::debug!("ignoring unsupported app-gateway thread item in TUI adapter");
            None
        }
    }
}

#[cfg(test)]
fn command_execution_started_event(turn_id: &str, item: &ThreadItem) -> Option<Vec<Event>> {
    let ThreadItem::CommandExecution {
        id,
        command,
        cwd,
        process_id,
        source,
        command_actions,
        ..
    } = item
    else {
        return None;
    };

    Some(vec![Event {
        id: String::new(),
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: id.clone(),
            process_id: process_id.clone(),
            turn_id: turn_id.to_string(),
            command: split_command_string(command),
            cwd: cwd.clone(),
            parsed_cmd: command_actions
                .iter()
                .cloned()
                .map(praxis_app_gateway_protocol::CommandAction::into_core)
                .collect(),
            source: source.to_core(),
            interaction_input: None,
        }),
    }])
}

#[cfg(test)]
fn command_execution_completed_event(turn_id: &str, item: &ThreadItem) -> Option<Vec<Event>> {
    let ThreadItem::CommandExecution {
        id,
        command,
        cwd,
        process_id,
        source,
        status,
        command_actions,
        aggregated_output,
        exit_code,
        duration_ms,
    } = item
    else {
        return None;
    };

    if matches!(
        status,
        praxis_app_gateway_protocol::CommandExecutionStatus::InProgress
    ) {
        return Some(Vec::new());
    }

    let status = match status {
        praxis_app_gateway_protocol::CommandExecutionStatus::InProgress => return Some(Vec::new()),
        praxis_app_gateway_protocol::CommandExecutionStatus::Completed => {
            ExecCommandStatus::Completed
        }
        praxis_app_gateway_protocol::CommandExecutionStatus::Failed => ExecCommandStatus::Failed,
        praxis_app_gateway_protocol::CommandExecutionStatus::Declined => {
            ExecCommandStatus::Declined
        }
    };

    let duration = Duration::from_millis(
        duration_ms
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_default(),
    );
    let aggregated_output = aggregated_output.clone().unwrap_or_default();

    Some(vec![Event {
        id: String::new(),
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: id.clone(),
            process_id: process_id.clone(),
            turn_id: turn_id.to_string(),
            command: split_command_string(command),
            cwd: cwd.clone(),
            parsed_cmd: command_actions
                .iter()
                .cloned()
                .map(praxis_app_gateway_protocol::CommandAction::into_core)
                .collect(),
            source: source.to_core(),
            interaction_input: None,
            stdout: String::new(),
            stderr: String::new(),
            aggregated_output: aggregated_output.clone(),
            exit_code: exit_code.unwrap_or(-1),
            duration,
            formatted_output: aggregated_output,
            status,
        }),
    }])
}

#[cfg(test)]
fn command_execution_snapshot_events(turn_id: &str, item: &ThreadItem) -> Option<Vec<Event>> {
    let mut events = command_execution_started_event(turn_id, item)?;
    if let Some(end_events) = command_execution_completed_event(turn_id, item) {
        events.extend(end_events);
    }
    Some(events)
}

#[cfg(test)]
fn app_gateway_praxis_error_info_to_core(
    value: praxis_app_gateway_protocol::PraxisErrorInfo,
) -> Option<praxis_protocol::protocol::PraxisErrorInfo> {
    serde_json::from_value(serde_json::to_value(value).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::command_execution_started_event;
    use super::server_notification_thread_events;
    use super::thread_snapshot_events;
    use super::turn_snapshot_events;
    use praxis_app_gateway_protocol::AgentMessageDeltaNotification;
    use praxis_app_gateway_protocol::CommandAction;
    use praxis_app_gateway_protocol::CommandExecutionSource;
    use praxis_app_gateway_protocol::CommandExecutionStatus;
    use praxis_app_gateway_protocol::ItemCompletedNotification;
    use praxis_app_gateway_protocol::PraxisErrorInfo;
    use praxis_app_gateway_protocol::ReasoningSummaryTextDeltaNotification;
    use praxis_app_gateway_protocol::ServerNotification;
    use praxis_app_gateway_protocol::Thread;
    use praxis_app_gateway_protocol::ThreadItem;
    use praxis_app_gateway_protocol::ThreadStatus;
    use praxis_app_gateway_protocol::Turn;
    use praxis_app_gateway_protocol::TurnCompletedNotification;
    use praxis_app_gateway_protocol::TurnError;
    use praxis_app_gateway_protocol::TurnStatus;
    use praxis_protocol::ThreadId;
    use praxis_protocol::items::AgentMessageContent;
    use praxis_protocol::items::AgentMessageItem;
    use praxis_protocol::items::TurnItem;
    use praxis_protocol::models::MessagePhase;
    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::ExecCommandSource;
    use praxis_protocol::protocol::SessionSource;
    use praxis_protocol::protocol::TurnAbortReason;
    use praxis_protocol::protocol::TurnAbortedEvent;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn bridges_completed_agent_messages_from_server_notifications() {
        let thread_id = "019cee8c-b993-7e33-88c0-014d4e62612d".to_string();
        let turn_id = "019cee8c-b9b4-7f10-a1b0-38caa876a012".to_string();
        let item_id = "msg_123".to_string();

        let (actual_thread_id, events) = server_notification_thread_events(
            ServerNotification::ItemCompleted(ItemCompletedNotification {
                item: ThreadItem::AgentMessage {
                    id: item_id,
                    text: "Hello from your coding assistant.".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
            }),
        )
        .expect("notification should bridge");

        assert_eq!(
            actual_thread_id,
            ThreadId::from_string(&thread_id).expect("valid thread id")
        );
        let [event] = events.as_slice() else {
            panic!("expected one bridged event");
        };
        assert_eq!(event.id, String::new());
        let EventMsg::ItemCompleted(completed) = &event.msg else {
            panic!("expected item completed event");
        };
        assert_eq!(
            completed.thread_id,
            ThreadId::from_string(&thread_id).expect("valid thread id")
        );
        assert_eq!(completed.turn_id, turn_id);
        match &completed.item {
            TurnItem::AgentMessage(AgentMessageItem {
                id, content, phase, ..
            }) => {
                assert_eq!(id, "msg_123");
                let [AgentMessageContent::Text { text }] = content.as_slice() else {
                    panic!("expected a single text content item");
                };
                assert_eq!(text, "Hello from your coding assistant.");
                assert_eq!(*phase, Some(MessagePhase::FinalAnswer));
            }
            _ => panic!("expected bridged agent message item"),
        }
    }

    #[test]
    fn bridges_turn_completion_from_server_notifications() {
        let thread_id = "019cee8c-b993-7e33-88c0-014d4e62612d".to_string();
        let turn_id = "019cee8c-b9b4-7f10-a1b0-38caa876a012".to_string();

        let (actual_thread_id, events) = server_notification_thread_events(
            ServerNotification::TurnCompleted(TurnCompletedNotification {
                thread_id: thread_id.clone(),
                turn: Turn {
                    id: turn_id.clone(),
                    items: Vec::new(),
                    status: TurnStatus::Completed,
                    error: None,
                },
            }),
        )
        .expect("notification should bridge");

        assert_eq!(
            actual_thread_id,
            ThreadId::from_string(&thread_id).expect("valid thread id")
        );
        let [event] = events.as_slice() else {
            panic!("expected one bridged event");
        };
        assert_eq!(event.id, String::new());
        let EventMsg::TurnComplete(completed) = &event.msg else {
            panic!("expected turn complete event");
        };
        assert_eq!(completed.turn_id, turn_id);
        assert_eq!(completed.last_agent_message, None);
    }

    #[test]
    fn command_execution_snapshot_preserves_non_roundtrippable_command_strings() {
        let item = ThreadItem::CommandExecution {
            id: "cmd-1".to_string(),
            command: r#"C:\Program Files\Git\bin\bash.exe -lc "echo hi""#.to_string(),
            cwd: PathBuf::from("C:\\repo"),
            process_id: None,
            source: CommandExecutionSource::UserShell,
            status: CommandExecutionStatus::InProgress,
            command_actions: vec![],
            aggregated_output: None,
            exit_code: None,
            duration_ms: None,
        };

        let events =
            command_execution_started_event("turn-1", &item).expect("command execution start");
        let [started] = events.as_slice() else {
            panic!("expected one started event");
        };
        let EventMsg::ExecCommandBegin(begin) = &started.msg else {
            panic!("expected exec begin event");
        };
        assert_eq!(
            begin.command,
            vec![r#"C:\Program Files\Git\bin\bash.exe -lc "echo hi""#.to_string()]
        );
    }

    #[test]
    fn replays_command_execution_items_from_thread_snapshots() {
        let thread = Thread {
            id: "019cee8c-b993-7e33-88c0-014d4e62612d".to_string(),
            preview: String::new(),
            summary: None,
            ephemeral: false,
            model_provider: "openai".to_string(),
            model: None,
            created_at: 1,
            updated_at: 1,
            status: ThreadStatus::Idle,
            path: None,
            cwd: PathBuf::from("/tmp"),
            cli_version: "test".to_string(),
            source: SessionSource::Cli.into(),
            agent_base_name: None,
            agent_title: None,
            agent_display_name: None,
            agent_role: None,
            git_info: None,
            name: None,
            total_cost_usd: None,
            last_cost_usd: None,
            token_usage: None,
            control_state: None,
            selfwork_plan_path: None,
            turns: vec![Turn {
                id: "turn-1".to_string(),
                items: vec![ThreadItem::CommandExecution {
                    id: "cmd-1".to_string(),
                    command: "printf 'hello world\\n'".to_string(),
                    cwd: PathBuf::from("/tmp"),
                    process_id: None,
                    source: CommandExecutionSource::UserShell,
                    status: CommandExecutionStatus::Completed,
                    command_actions: vec![CommandAction::Unknown {
                        command: "printf hello world".to_string(),
                    }],
                    aggregated_output: Some("hello world\n".to_string()),
                    exit_code: Some(0),
                    duration_ms: Some(5),
                }],
                status: TurnStatus::Completed,
                error: None,
            }],
        };

        let events = thread_snapshot_events(&thread, /*show_raw_agent_reasoning*/ false);
        assert!(matches!(events[0].msg, EventMsg::TurnStarted(_)));
        let EventMsg::ExecCommandBegin(begin) = &events[1].msg else {
            panic!("expected exec begin event");
        };
        assert_eq!(begin.call_id, "cmd-1");
        assert_eq!(begin.source, ExecCommandSource::UserShell);
        let EventMsg::ExecCommandEnd(end) = &events[2].msg else {
            panic!("expected exec end event");
        };
        assert_eq!(end.call_id, "cmd-1");
        assert_eq!(end.formatted_output, "hello world\n");
        assert!(matches!(events[3].msg, EventMsg::TurnComplete(_)));
    }

    #[test]
    fn bridges_interrupted_turn_completion_from_server_notifications() {
        let thread_id = "019cee8c-b993-7e33-88c0-014d4e62612d".to_string();
        let turn_id = "019cee8c-b9b4-7f10-a1b0-38caa876a012".to_string();

        let (actual_thread_id, events) = server_notification_thread_events(
            ServerNotification::TurnCompleted(TurnCompletedNotification {
                thread_id: thread_id.clone(),
                turn: Turn {
                    id: turn_id.clone(),
                    items: Vec::new(),
                    status: TurnStatus::Interrupted,
                    error: None,
                },
            }),
        )
        .expect("notification should bridge");

        assert_eq!(
            actual_thread_id,
            ThreadId::from_string(&thread_id).expect("valid thread id")
        );
        let [event] = events.as_slice() else {
            panic!("expected one bridged event");
        };
        let EventMsg::TurnAborted(aborted) = &event.msg else {
            panic!("expected turn aborted event");
        };
        assert_eq!(aborted.turn_id.as_deref(), Some(turn_id.as_str()));
        assert_eq!(aborted.reason, TurnAbortReason::Interrupted);
    }

    #[test]
    fn bridges_failed_turn_completion_from_server_notifications() {
        let thread_id = "019cee8c-b993-7e33-88c0-014d4e62612d".to_string();
        let turn_id = "019cee8c-b9b4-7f10-a1b0-38caa876a012".to_string();

        let (actual_thread_id, events) = server_notification_thread_events(
            ServerNotification::TurnCompleted(TurnCompletedNotification {
                thread_id: thread_id.clone(),
                turn: Turn {
                    id: turn_id.clone(),
                    items: Vec::new(),
                    status: TurnStatus::Failed,
                    error: Some(TurnError {
                        message: "request failed".to_string(),
                        praxis_error_info: Some(PraxisErrorInfo::Other),
                        additional_details: None,
                    }),
                },
            }),
        )
        .expect("notification should bridge");

        assert_eq!(
            actual_thread_id,
            ThreadId::from_string(&thread_id).expect("valid thread id")
        );
        let [complete_event] = events.as_slice() else {
            panic!("expected turn completion only");
        };
        let EventMsg::TurnComplete(completed) = &complete_event.msg else {
            panic!("expected turn complete event");
        };
        assert_eq!(completed.turn_id, turn_id);
        assert_eq!(completed.last_agent_message, None);
    }

    #[test]
    fn bridges_text_deltas_from_server_notifications() {
        let thread_id = "019cee8c-b993-7e33-88c0-014d4e62612d".to_string();

        let (_, agent_events) = server_notification_thread_events(
            ServerNotification::AgentMessageDelta(AgentMessageDeltaNotification {
                thread_id: thread_id.clone(),
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: "Hello".to_string(),
            }),
        )
        .expect("notification should bridge");
        let [agent_event] = agent_events.as_slice() else {
            panic!("expected one bridged agent delta event");
        };
        assert_eq!(agent_event.id, String::new());
        let EventMsg::AgentMessageDelta(delta) = &agent_event.msg else {
            panic!("expected bridged agent message delta");
        };
        assert_eq!(delta.delta, "Hello");

        let (_, reasoning_events) = server_notification_thread_events(
            ServerNotification::ReasoningSummaryTextDelta(ReasoningSummaryTextDeltaNotification {
                thread_id,
                turn_id: "turn".to_string(),
                item_id: "item".to_string(),
                delta: "reasoning delta".to_string(),
                summary_index: 0,
            }),
        )
        .expect("notification should bridge");
        let [reasoning_event] = reasoning_events.as_slice() else {
            panic!("expected one bridged reasoning delta event");
        };
        assert_eq!(reasoning_event.id, String::new());
        let EventMsg::AgentReasoningDelta(delta) = &reasoning_event.msg else {
            panic!("expected bridged reasoning delta");
        };
        assert_eq!(delta.delta, "reasoning delta");
    }

    #[test]
    fn bridges_thread_snapshot_turns_for_resume_restore() {
        let thread_id = ThreadId::new();
        let events = thread_snapshot_events(
            &Thread {
                id: thread_id.to_string(),
                preview: "hello".to_string(),
                summary: None,
                ephemeral: false,
                model_provider: "openai".to_string(),
                model: None,
                created_at: 0,
                updated_at: 0,
                status: ThreadStatus::Idle,
                path: None,
                cwd: PathBuf::from("/tmp/project"),
                cli_version: "test".to_string(),
                source: SessionSource::Cli.into(),
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                git_info: None,
                name: Some("restore".to_string()),
                total_cost_usd: None,
                last_cost_usd: None,
                token_usage: None,
                control_state: None,
                selfwork_plan_path: None,
                turns: vec![
                    Turn {
                        id: "turn-complete".to_string(),
                        items: vec![
                            ThreadItem::UserMessage {
                                id: "user-1".to_string(),
                                content: vec![praxis_app_gateway_protocol::UserInput::Text {
                                    text: "hello".to_string(),
                                    text_elements: Vec::new(),
                                }],
                            },
                            ThreadItem::AgentMessage {
                                id: "assistant-1".to_string(),
                                text: "hi".to_string(),
                                phase: Some(MessagePhase::FinalAnswer),
                                memory_citation: None,
                            },
                        ],
                        status: TurnStatus::Completed,
                        error: None,
                    },
                    Turn {
                        id: "turn-interrupted".to_string(),
                        items: Vec::new(),
                        status: TurnStatus::Interrupted,
                        error: None,
                    },
                    Turn {
                        id: "turn-failed".to_string(),
                        items: Vec::new(),
                        status: TurnStatus::Failed,
                        error: Some(TurnError {
                            message: "request failed".to_string(),
                            praxis_error_info: Some(PraxisErrorInfo::Other),
                            additional_details: None,
                        }),
                    },
                ],
            },
            /*show_raw_agent_reasoning*/ false,
        );

        assert_eq!(events.len(), 9);
        assert!(matches!(events[0].msg, EventMsg::TurnStarted(_)));
        assert!(matches!(events[1].msg, EventMsg::ItemCompleted(_)));
        assert!(matches!(events[2].msg, EventMsg::ItemCompleted(_)));
        assert!(matches!(events[3].msg, EventMsg::TurnComplete(_)));
        assert!(matches!(events[4].msg, EventMsg::TurnStarted(_)));
        let EventMsg::TurnAborted(TurnAbortedEvent { turn_id, reason }) = &events[5].msg else {
            panic!("expected interrupted turn replay");
        };
        assert_eq!(turn_id.as_deref(), Some("turn-interrupted"));
        assert_eq!(*reason, TurnAbortReason::Interrupted);
        assert!(matches!(events[6].msg, EventMsg::TurnStarted(_)));
        let EventMsg::Error(error) = &events[7].msg else {
            panic!("expected failed turn error replay");
        };
        assert_eq!(error.message, "request failed");
        assert_eq!(
            error.praxis_error_info,
            Some(praxis_protocol::protocol::PraxisErrorInfo::Other)
        );
        assert!(matches!(events[8].msg, EventMsg::TurnComplete(_)));
    }

    #[test]
    fn bridges_raw_reasoning_snapshot_items_when_enabled() {
        let events = turn_snapshot_events(
            ThreadId::new(),
            &Turn {
                id: "turn-complete".to_string(),
                items: vec![ThreadItem::Reasoning {
                    id: "reasoning-1".to_string(),
                    summary: vec!["Need to inspect config".to_string()],
                    content: vec!["hidden chain".to_string()],
                }],
                status: TurnStatus::Completed,
                error: None,
            },
            /*show_raw_agent_reasoning*/ true,
        );

        assert_eq!(events.len(), 4);
        assert!(matches!(events[0].msg, EventMsg::TurnStarted(_)));
        let EventMsg::AgentReasoning(reasoning) = &events[1].msg else {
            panic!("expected reasoning replay");
        };
        assert_eq!(reasoning.text, "Need to inspect config");
        let EventMsg::AgentReasoningRawContent(raw_reasoning) = &events[2].msg else {
            panic!("expected raw reasoning replay");
        };
        assert_eq!(raw_reasoning.text, "hidden chain");
        assert!(matches!(events[3].msg, EventMsg::TurnComplete(_)));
    }
}
