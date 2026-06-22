use serde::Deserialize;
use serde::Serialize;

use crate::context::EffectivePermissions;
use crate::tool::ToolCall;
use crate::tool::ToolResult;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ToolDecision {
    Allow,
    Block(String),
    Modify(ToolCall),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ToolResultDecision {
    AsIs,
    Rewrite(ToolResult),
    Terminate(ToolResult),
}

#[derive(Clone, Copy, Debug)]
pub struct ToolCallView<'a> {
    pub call: &'a ToolCall,
    pub permissions: &'a EffectivePermissions,
}

#[derive(Clone, Copy, Debug)]
pub struct ToolResultView<'a> {
    pub call: &'a ToolCall,
    pub result: &'a ToolResult,
}
