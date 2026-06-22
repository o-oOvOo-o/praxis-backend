use std::collections::HashSet;

use praxis_protocol::models::ResponseItem;

#[derive(Debug)]
pub(in crate::praxis::turn_loop_adapter) struct TurnPrepareOutcome {
    pub(in crate::praxis::turn_loop_adapter) explicitly_enabled_connectors: HashSet<String>,
    pub(in crate::praxis::turn_loop_adapter) prepared_items: Vec<ResponseItem>,
}
