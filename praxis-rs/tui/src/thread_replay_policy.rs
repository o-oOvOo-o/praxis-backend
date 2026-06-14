use praxis_app_gateway_protocol::Turn;

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
