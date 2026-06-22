use super::super::*;

impl AgentControl {
    pub(in crate::agent::control) fn upgrade(&self) -> PraxisResult<Arc<ThreadManagerInner>> {
        self.manager
            .upgrade()
            .ok_or_else(|| PraxisErr::UnsupportedOperation("thread manager dropped".to_string()))
    }
}
