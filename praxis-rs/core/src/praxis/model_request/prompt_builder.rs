use std::collections::HashMap;
use std::collections::HashSet;

use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use praxis_protocol::protocol::TokenSavingEvent;
use praxis_protocol::protocol::TokenSavingKind;
use praxis_utils_output_truncation::approx_token_count;

use crate::client_common::Prompt;
use crate::praxis::TurnContext;
use crate::tools::ToolRouter;

const REPEATED_OUTPUT_MIN_TOKENS: usize = 128;

pub(crate) struct PromptBuildResult {
    pub(crate) prompt: Prompt,
    pub(crate) saving_events: Vec<TokenSavingEvent>,
}

pub(crate) fn build_prompt(
    mut input: Vec<ResponseItem>,
    router: &ToolRouter,
    turn_context: &TurnContext,
    base_instructions: BaseInstructions,
) -> PromptBuildResult {
    let deferred_dynamic_tools = turn_context
        .dynamic_tools
        .iter()
        .filter(|tool| tool.defer_loading)
        .map(|tool| tool.name.as_str())
        .collect::<HashSet<_>>();
    let all_tools = router.model_visible_specs();
    let mut saving_events = deferred_tool_schema_event(&all_tools, &deferred_dynamic_tools)
        .into_iter()
        .collect::<Vec<_>>();
    let mut tools = if deferred_dynamic_tools.is_empty() {
        all_tools
    } else {
        all_tools
            .into_iter()
            .filter(|spec| !deferred_dynamic_tools.contains(spec.name()))
            .collect()
    };
    tools.retain(|spec| !turn_context.tool_loop_guard.should_hide_tool(spec.name()));

    if let Some(event) = project_repeated_tool_outputs(&mut input) {
        saving_events.push(event);
    }
    if let Some(event) = project_plan_working_state(&mut input) {
        saving_events.push(event);
    }

    PromptBuildResult {
        prompt: Prompt {
            input,
            tools,
            parallel_tool_calls: turn_context.model_info.supports_parallel_tool_calls,
            base_instructions,
            personality: turn_context.personality,
            output_schema: turn_context.final_output_json_schema.clone(),
        },
        saving_events,
    }
}

fn deferred_tool_schema_event(
    tools: &[praxis_tools::ToolSpec],
    deferred_names: &HashSet<&str>,
) -> Option<TokenSavingEvent> {
    if deferred_names.is_empty() {
        return None;
    }
    let omitted = tools
        .iter()
        .filter(|tool| deferred_names.contains(tool.name()))
        .collect::<Vec<_>>();
    let original_tokens = serialized_token_count(&omitted);
    (original_tokens > 0).then(|| {
        TokenSavingEvent::new(
            TokenSavingKind::ToolSchemaElision,
            original_tokens,
            0,
            true,
            Some("dynamic-tools://deferred".to_string()),
        )
    })
}

fn project_repeated_tool_outputs(input: &mut [ResponseItem]) -> Option<TokenSavingEvent> {
    let original_tokens = serialized_token_count(input);
    let mut first_outputs = HashMap::<(Option<bool>, String), String>::new();

    for item in input.iter_mut() {
        let (call_id, output) = match item {
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => (call_id, output),
            _ => continue,
        };
        if output.success == Some(false) {
            continue;
        }
        let success = output.success;
        let FunctionCallOutputBody::Text(text) = &mut output.body else {
            continue;
        };
        if approx_token_count(text) < REPEATED_OUTPUT_MIN_TOKENS {
            continue;
        }
        let key = (success, text.clone());
        if let Some(first_call_id) = first_outputs.get(&key) {
            *text = format!(
                "<praxis-reference kind=\"unchanged-tool-output\" source_call_id=\"{first_call_id}\" />"
            );
        } else {
            first_outputs.insert(key, call_id.clone());
        }
    }

    savings_event(
        TokenSavingKind::UnchangedResource,
        original_tokens,
        serialized_token_count(input),
        "prompt://unchanged-tool-outputs",
    )
}

fn project_plan_working_state(input: &mut Vec<ResponseItem>) -> Option<TokenSavingEvent> {
    let updates = input
        .iter()
        .filter_map(|item| match item {
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } if name == "update_plan" => serde_json::from_str::<UpdatePlanArgs>(arguments)
                .ok()
                .map(|args| (call_id.clone(), args)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if updates.len() < 2 {
        return None;
    }

    let mut step_ids = HashMap::<String, usize>::new();
    let mut steps = Vec::<String>::new();
    let encoded_updates = updates
        .iter()
        .map(|(call_id, update)| {
            let plan = update
                .plan
                .iter()
                .map(|item| {
                    let id = *step_ids.entry(item.step.clone()).or_insert_with(|| {
                        let id = steps.len();
                        steps.push(item.step.clone());
                        id
                    });
                    serde_json::json!({ "step": id, "status": item.status })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "call_id": call_id,
                "explanation": update.explanation,
                "plan": plan,
            })
        })
        .collect::<Vec<_>>();
    let state = serde_json::json!({
        "format": "praxis.plan-state.v1",
        "steps": steps,
        "updates": encoded_updates,
    });
    let state_text = format!(
        "<praxis-working-state>\n{}\n</praxis-working-state>",
        serde_json::to_string(&state).ok()?
    );

    let original_tokens = serialized_token_count(input);
    let mut projected = input.clone();
    let projected_call_ids = updates
        .iter()
        .map(|(call_id, _)| call_id.as_str())
        .collect::<HashSet<_>>();
    for item in &mut projected {
        match item {
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } if name == "update_plan" && projected_call_ids.contains(call_id.as_str()) => {
                *arguments = "{\"projected_into\":\"praxis-working-state\"}".to_string();
            }
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } if projected_call_ids.contains(call_id.as_str()) => {
                output.body = FunctionCallOutputBody::Text(
                    "<praxis-reference kind=\"working-state\" />".to_string(),
                );
            }
            _ => {}
        }
    }
    projected.push(ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::InputText { text: state_text }],
        end_turn: None,
        phase: None,
    });
    let sent_tokens = serialized_token_count(&projected);
    let event = savings_event(
        TokenSavingKind::WorkingStateProjection,
        original_tokens,
        sent_tokens,
        "prompt://working-state/plan",
    )?;
    *input = projected;
    Some(event)
}

fn savings_event(
    kind: TokenSavingKind,
    original_tokens: i64,
    sent_tokens: i64,
    reference: &str,
) -> Option<TokenSavingEvent> {
    (original_tokens > sent_tokens).then(|| {
        TokenSavingEvent::new(
            kind,
            original_tokens,
            sent_tokens,
            true,
            Some(reference.to_string()),
        )
    })
}

fn serialized_token_count<T: serde::Serialize + ?Sized>(value: &T) -> i64 {
    serde_json::to_string(value)
        .ok()
        .and_then(|text| i64::try_from(approx_token_count(&text)).ok())
        .unwrap_or(0)
}
