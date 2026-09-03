use praxis_thread_store_contracts::CommandId;
use std::collections::HashMap;
use std::collections::VecDeque;

const FILTER_WORDS: usize = 1 << 15;
const FILTER_MASK: usize = FILTER_WORDS * u64::BITS as usize - 1;
const RECENT_COMMANDS: usize = 512;
const HASH_STEP: u64 = 0x9e37_79b9_7f4a_7c15;

pub(crate) struct CommandIndex {
    filter: Option<Box<[u64]>>,
    recent: HashMap<CommandId, usize>,
    order: VecDeque<CommandId>,
}

impl CommandIndex {
    pub(crate) fn new() -> Self {
        Self {
            filter: None,
            recent: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn insert(&mut self, command_id: CommandId, frame_index: usize) {
        let filter = self
            .filter
            .get_or_insert_with(|| vec![0; FILTER_WORDS].into_boxed_slice());
        for bit in command_bits(command_id) {
            filter[bit / u64::BITS as usize] |= 1_u64 << (bit % u64::BITS as usize);
        }
        if self.recent.insert(command_id, frame_index).is_some() {
            return;
        }
        self.order.push_back(command_id);
        if self.order.len() > RECENT_COMMANDS
            && let Some(expired) = self.order.pop_front()
        {
            self.recent.remove(&expired);
        }
    }

    pub(crate) fn maybe_contains(&self, command_id: CommandId) -> bool {
        let Some(filter) = self.filter.as_ref() else {
            return false;
        };
        command_bits(command_id).into_iter().all(|bit| {
            filter[bit / u64::BITS as usize] & (1_u64 << (bit % u64::BITS as usize)) != 0
        })
    }

    pub(crate) fn recent_frame(&self, command_id: CommandId) -> Option<usize> {
        self.recent.get(&command_id).copied()
    }
}

fn command_bits(command_id: CommandId) -> [usize; 4] {
    let value = command_id.as_uuid().as_u128();
    let low = value as u64;
    let high = (value >> u64::BITS) as u64;
    let first = mix(low ^ high.rotate_left(17));
    let step = mix(high ^ low.rotate_right(11) ^ HASH_STEP) | 1;
    std::array::from_fn(|index| {
        first
            .wrapping_add((index as u64).wrapping_mul(step))
            .wrapping_add(
                (index as u64)
                    .wrapping_mul(index as u64)
                    .wrapping_mul(HASH_STEP),
            ) as usize
            & FILTER_MASK
    })
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
