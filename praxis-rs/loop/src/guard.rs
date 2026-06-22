use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LoopGuard {
    #[serde(rename = "max_tool_calls")]
    tool_call_limit: ToolCallLimit,
}

impl LoopGuard {
    pub fn with_max_tool_calls(max_tool_calls: u64) -> Self {
        Self {
            tool_call_limit: ToolCallLimit::capped(max_tool_calls),
        }
    }

    pub fn admit_tool_calls(&self, attempted_tool_call_count: u64) -> ToolCallAdmission {
        if self.tool_call_limit.allows(attempted_tool_call_count) {
            ToolCallAdmission::Accepted
        } else {
            ToolCallAdmission::Rejected {
                attempted_tool_call_count,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallAdmission {
    Accepted,
    Rejected { attempted_tool_call_count: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallLimit {
    Unlimited,
    Capped { max_tool_calls: u64 },
}

impl Default for ToolCallLimit {
    fn default() -> Self {
        Self::Unlimited
    }
}

impl Serialize for ToolCallLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Unlimited => serializer.serialize_none(),
            Self::Capped { max_tool_calls } => serializer.serialize_some(max_tool_calls),
        }
    }
}

impl<'de> Deserialize<'de> for ToolCallLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<u64>::deserialize(deserializer).map(Self::from_optional_cap)
    }
}

impl ToolCallLimit {
    pub fn unlimited() -> Self {
        Self::Unlimited
    }

    pub fn capped(max_tool_calls: u64) -> Self {
        Self::Capped { max_tool_calls }
    }

    fn allows(self, next_tool_call_count: u64) -> bool {
        match self {
            Self::Unlimited => true,
            Self::Capped { max_tool_calls } => next_tool_call_count <= max_tool_calls,
        }
    }

    fn from_optional_cap(max_tool_calls: Option<u64>) -> Self {
        match max_tool_calls {
            Some(max_tool_calls) => Self::Capped { max_tool_calls },
            None => Self::Unlimited,
        }
    }
}
