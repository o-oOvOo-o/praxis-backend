use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::ids::ThreadId;
use crate::ids::TraceId;
use crate::ids::TurnId;
use crate::model::ModelSpec;
use crate::model::PromptItem;
use crate::tool::ToolSpec;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnContext {
    pub turn_id: TurnId,
    pub thread_id: ThreadId,
    pub trace_id: TraceId,
    pub model: ModelSpec,
    pub reasoning: Option<String>,
    pub service_tier: Option<String>,
    pub permissions: EffectivePermissions,
    pub collaboration_mode: CollaborationMode,
    pub cwd: Option<PathBuf>,
    pub tools: Vec<ToolSpec>,
    pub features: TurnFeatures,
    pub initial_prompt_items: Vec<PromptItem>,
}

impl TurnContext {
    pub fn new(turn_id: TurnId, thread_id: ThreadId, trace_id: TraceId, model: ModelSpec) -> Self {
        Self {
            turn_id,
            thread_id,
            trace_id,
            model,
            reasoning: None,
            service_tier: None,
            permissions: EffectivePermissions::default(),
            collaboration_mode: CollaborationMode::default(),
            cwd: None,
            tools: Vec::new(),
            features: TurnFeatures::default(),
            initial_prompt_items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnInput {
    pub prompt_items: Vec<PromptItem>,
}

impl TurnInput {
    pub fn from_prompt_items(prompt_items: Vec<PromptItem>) -> Self {
        Self { prompt_items }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectivePermissions {
    pub write: bool,
    pub network: bool,
    pub approval_required: bool,
}

impl Default for EffectivePermissions {
    fn default() -> Self {
        Self {
            write: false,
            network: false,
            approval_required: true,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum CollaborationMode {
    ReadOnly,
    WorkspaceWrite,
    #[default]
    FullAccess,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnFeatures {
    pub streaming: bool,
    pub tool_calls: bool,
}

impl Default for TurnFeatures {
    fn default() -> Self {
        Self {
            streaming: true,
            tool_calls: true,
        }
    }
}
