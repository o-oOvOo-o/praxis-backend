use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::Turn;
use praxis_app_gateway_protocol::TurnStatus;

const VISIBLE_REPLAY_TURN_LIMIT: usize = 64;
const VISIBLE_REPLAY_ITEM_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VisibleReplayBudget {
    max_turns: usize,
    max_items: usize,
}

impl Default for VisibleReplayBudget {
    fn default() -> Self {
        Self {
            max_turns: VISIBLE_REPLAY_TURN_LIMIT,
            max_items: VISIBLE_REPLAY_ITEM_LIMIT,
        }
    }
}

pub(crate) fn compact_visible_replay_turns(turns: Vec<Turn>) -> Vec<Turn> {
    VisibleReplayBudget::default().compact(turns)
}

pub(crate) fn compact_conversation_replay_turns(turns: Vec<Turn>) -> Vec<Turn> {
    let turns = turns
        .into_iter()
        .filter_map(|mut turn| {
            turn.items.retain(is_conversation_item);
            (!turn.items.is_empty() || matches!(turn.status, TurnStatus::InProgress))
                .then_some(turn)
        })
        .collect();
    VisibleReplayBudget::default().compact(turns)
}

fn is_conversation_item(item: &ThreadItem) -> bool {
    matches!(
        item,
        ThreadItem::UserMessage { .. } | ThreadItem::AgentMessage { .. }
    )
}

impl VisibleReplayBudget {
    fn compact(self, turns: Vec<Turn>) -> Vec<Turn> {
        if turns.len() <= self.max_turns
            && turns.iter().map(|turn| turn.items.len()).sum::<usize>() <= self.max_items
        {
            return turns;
        }

        let mut item_count = 0usize;
        let mut kept = Vec::new();
        for mut turn in turns.into_iter().rev().take(self.max_turns) {
            let remaining = self.max_items.saturating_sub(item_count);
            if remaining == 0 {
                break;
            }
            if turn.items.len() > remaining {
                let skip = turn.items.len().saturating_sub(remaining);
                turn.items = turn.items.into_iter().skip(skip).collect();
            }
            item_count = item_count.saturating_add(turn.items.len());
            kept.push(turn);
        }
        kept.reverse();
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_app_gateway_protocol::CommandExecutionSource;
    use praxis_app_gateway_protocol::CommandExecutionStatus;
    use praxis_app_gateway_protocol::MessagePhase;
    use praxis_app_gateway_protocol::UserInput;
    use std::path::PathBuf;

    fn completed_turn(items: Vec<ThreadItem>) -> Turn {
        Turn {
            id: "turn-1".to_string(),
            items,
            status: TurnStatus::Completed,
            error: None,
        }
    }

    #[test]
    fn conversation_replay_keeps_user_and_assistant_messages_only() {
        let turns = compact_conversation_replay_turns(vec![completed_turn(vec![
            ThreadItem::UserMessage {
                id: "user-1".to_string(),
                content: vec![UserInput::Text {
                    text: "question".to_string(),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::CommandExecution {
                id: "command-1".to_string(),
                command: "cargo run".to_string(),
                cwd: PathBuf::from("D:/ghost1.0"),
                process_id: None,
                source: CommandExecutionSource::UnifiedExecStartup,
                status: CommandExecutionStatus::Completed,
                command_actions: Vec::new(),
                aggregated_output: Some("done".to_string()),
                exit_code: Some(0),
                duration_ms: Some(1),
            },
            ThreadItem::AgentMessage {
                id: "assistant-1".to_string(),
                text: "answer".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
        ])]);

        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].items.len(), 2);
        assert!(matches!(turns[0].items[0], ThreadItem::UserMessage { .. }));
        assert!(matches!(turns[0].items[1], ThreadItem::AgentMessage { .. }));
    }

    #[test]
    fn conversation_replay_drops_completed_tool_only_turns() {
        let turns =
            compact_conversation_replay_turns(vec![completed_turn(vec![ThreadItem::Plan {
                id: "plan-1".to_string(),
                text: "internal plan".to_string(),
            }])]);

        assert!(turns.is_empty());
    }

    #[test]
    fn conversation_replay_preserves_empty_in_progress_turn_state() {
        let turns = compact_conversation_replay_turns(vec![Turn {
            id: "turn-running".to_string(),
            items: vec![ThreadItem::Plan {
                id: "plan-1".to_string(),
                text: "internal plan".to_string(),
            }],
            status: TurnStatus::InProgress,
            error: None,
        }]);

        assert_eq!(turns.len(), 1);
        assert!(turns[0].items.is_empty());
    }
}
