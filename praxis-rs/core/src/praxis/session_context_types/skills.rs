use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::capabilities::ResolvedSkillsCapability;

#[derive(Clone, Debug)]
pub(crate) struct TurnSkillsContext {
    pub(crate) outcome: ResolvedSkillsCapability,
    pub(crate) implicit_invocation_seen_skills: Arc<Mutex<HashSet<String>>>,
}

impl TurnSkillsContext {
    pub(crate) fn new(outcome: ResolvedSkillsCapability) -> Self {
        Self {
            outcome,
            implicit_invocation_seen_skills: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}
