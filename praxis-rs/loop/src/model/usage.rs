use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub total: u64,
    pub reasoning: u64,
}

impl TokenUsage {
    pub fn add_assign(&mut self, usage: &TokenUsage) {
        self.input += usage.input;
        self.output += usage.output;
        self.total += usage.total;
        self.reasoning += usage.reasoning;
    }
}
