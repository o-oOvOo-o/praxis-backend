use crate::context::TurnContext;
use crate::context::TurnInput;
use crate::model::PromptItem;
use crate::model::TurnItem;
use crate::state::TurnState;

pub(crate) fn build_initial_prompt(ctx: &TurnContext, input: &TurnInput) -> Vec<PromptItem> {
    let mut prompt = ctx.initial_prompt_items.clone();
    prompt.extend(input.prompt_items.clone());
    prompt
}

pub(crate) fn build_round_prompt(prompt_base: &[PromptItem], state: &TurnState) -> Vec<PromptItem> {
    let mut prompt = prompt_base.to_vec();
    for item in state.transcript_delta() {
        prompt_projection_from_turn_item(item).append_to(&mut prompt);
    }
    prompt
}

enum PromptProjection {
    Include(PromptItem),
    RuntimeOnly,
}

impl PromptProjection {
    fn append_to(self, prompt: &mut Vec<PromptItem>) {
        match self {
            Self::Include(item) => prompt.push(item),
            Self::RuntimeOnly => {}
        }
    }
}

fn prompt_projection_from_turn_item(item: &TurnItem) -> PromptProjection {
    match item {
        TurnItem::AssistantText { text, .. } => {
            PromptProjection::Include(PromptItem::AssistantText(text.clone()))
        }
        TurnItem::Reasoning { .. } => PromptProjection::RuntimeOnly,
        TurnItem::ToolCall(call) => PromptProjection::Include(PromptItem::ToolCall {
            call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        }),
        TurnItem::ToolStarted { .. } | TurnItem::ToolProgress { .. } => {
            PromptProjection::RuntimeOnly
        }
        TurnItem::ToolResult(result) => PromptProjection::Include(PromptItem::ToolResult {
            call_id: result.call_id.clone(),
            content: result.content.clone(),
            status: result.status,
        }),
        TurnItem::SystemText(text) => {
            PromptProjection::Include(PromptItem::SystemText(text.clone()))
        }
        TurnItem::UserText(text) => PromptProjection::Include(PromptItem::UserText(text.clone())),
    }
}
