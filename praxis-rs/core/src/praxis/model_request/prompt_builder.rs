use std::collections::HashSet;
use std::io;
use std::io::Write;

use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::plan_tool::UpdatePlanArgs;
use praxis_protocol::protocol::TokenSavingEvent;
use praxis_protocol::protocol::TokenSavingKind;
use praxis_utils_output_truncation::approx_token_count;
use praxis_utils_output_truncation::approx_tokens_from_byte_count;

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

    saving_events.extend(project_repeated_tool_outputs(&mut input));
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

fn project_repeated_tool_outputs(input: &mut [ResponseItem]) -> Vec<TokenSavingEvent> {
    let mut canonical_outputs = Vec::<(Option<bool>, String, usize)>::new();
    let mut saved_exact_tokens: usize = 0;
    let mut saved_delta_tokens: usize = 0;

    for item_index in 0..input.len() {
        let (previous_items, current_items) = input.split_at_mut(item_index);
        let item = &mut current_items[0];
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
        let original = text.as_str();
        let projection = canonical_outputs
            .iter()
            .find_map(|(candidate_success, source_call_id, candidate_index)| {
                let candidate = successful_tool_output_text(&previous_items[*candidate_index])?;
                (*candidate_success == success && candidate == original).then(|| {
                    (
                        TokenSavingKind::UnchangedResource,
                        format!(
                            "<praxis-reference kind=\"unchanged-tool-output\" source_call_id=\"{source_call_id}\" />"
                        ),
                    )
                })
            })
            .or_else(|| {
                canonical_outputs
                    .iter()
                    .filter_map(|(candidate_success, source_call_id, candidate_index)| {
                        let candidate =
                            successful_tool_output_text(&previous_items[*candidate_index])?;
                        (*candidate_success == success
                            && original.len() > candidate.len()
                            && original.starts_with(candidate))
                        .then_some((source_call_id, candidate))
                    })
                    .max_by_key(|(_, candidate)| candidate.len())
                    .map(|(source_call_id, candidate)| {
                        (
                            TokenSavingKind::OutputDelta,
                            format!(
                                "<praxis-reference kind=\"append-only-tool-output\" source_call_id=\"{source_call_id}\" />\n{}",
                                &original[candidate.len()..]
                            ),
                        )
                    })
            });
        let Some((kind, projected)) = projection else {
            canonical_outputs.push((success, call_id.clone(), item_index));
            continue;
        };
        let saved_tokens =
            approx_token_count(original).saturating_sub(approx_token_count(&projected));
        if saved_tokens == 0 {
            canonical_outputs.push((success, call_id.clone(), item_index));
            continue;
        }
        *text = projected;
        match kind {
            TokenSavingKind::UnchangedResource => {
                saved_exact_tokens = saved_exact_tokens.saturating_add(saved_tokens);
            }
            TokenSavingKind::OutputDelta => {
                saved_delta_tokens = saved_delta_tokens.saturating_add(saved_tokens);
            }
            _ => unreachable!("tool output projection emitted an unsupported saving kind"),
        }
    }

    [
        (
            TokenSavingKind::UnchangedResource,
            saved_exact_tokens,
            "prompt://unchanged-tool-outputs",
        ),
        (
            TokenSavingKind::OutputDelta,
            saved_delta_tokens,
            "prompt://append-only-tool-output-deltas",
        ),
    ]
    .into_iter()
    .filter(|(_, saved_tokens, _)| *saved_tokens > 0)
    .map(|(kind, saved_tokens, reference)| {
        TokenSavingEvent::reversible(kind, saved_tokens as i64, Some(reference.to_string()))
    })
    .collect()
}

fn successful_tool_output_text(item: &ResponseItem) -> Option<&str> {
    let output = match item {
        ResponseItem::FunctionCallOutput { output, .. }
        | ResponseItem::CustomToolCallOutput { output, .. } => output,
        _ => return None,
    };
    if output.success == Some(false) {
        return None;
    }
    match &output.body {
        FunctionCallOutputBody::Text(text) => Some(text),
        FunctionCallOutputBody::ContentItems(_) => None,
    }
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

    let (call_id, update) = updates.last()?;
    let state = serde_json::json!({
        "format": "praxis.plan-state.v2",
        "call_id": call_id,
        "explanation": update.explanation,
        "plan": update.plan,
    });
    let state_text = format!(
        "<praxis-working-state>\n{}\n</praxis-working-state>",
        serde_json::to_string(&state).ok()?
    );

    let original_tokens = serialized_token_count(input);
    let projected_call_ids = updates
        .iter()
        .map(|(call_id, _)| call_id.as_str())
        .collect::<HashSet<_>>();
    let mut rollback = Vec::new();
    for (item_index, item) in input.iter_mut().enumerate() {
        match item {
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } if name == "update_plan" && projected_call_ids.contains(call_id.as_str()) => {
                rollback.push(PlanProjectionRollback::Arguments {
                    item_index,
                    value: std::mem::replace(
                        arguments,
                        "{\"projected_into\":\"praxis-working-state\"}".to_string(),
                    ),
                });
            }
            ResponseItem::FunctionCallOutput { call_id, output }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } if projected_call_ids.contains(call_id.as_str()) => {
                rollback.push(PlanProjectionRollback::Output {
                    item_index,
                    body: std::mem::replace(
                        &mut output.body,
                        FunctionCallOutputBody::Text(
                            "<praxis-reference kind=\"working-state\" />".to_string(),
                        ),
                    ),
                });
            }
            _ => {}
        }
    }
    input.push(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText { text: state_text }],
        end_turn: None,
        phase: None,
    });
    let sent_tokens = serialized_token_count(input);
    let event = savings_event(
        TokenSavingKind::WorkingStateProjection,
        original_tokens,
        sent_tokens,
        "prompt://working-state/plan",
    );
    if event.is_none() {
        input.pop();
        restore_plan_projection(input, rollback);
    }
    event
}

enum PlanProjectionRollback {
    Arguments {
        item_index: usize,
        value: String,
    },
    Output {
        item_index: usize,
        body: FunctionCallOutputBody,
    },
}

fn restore_plan_projection(input: &mut [ResponseItem], rollback: Vec<PlanProjectionRollback>) {
    for entry in rollback {
        match entry {
            PlanProjectionRollback::Arguments { item_index, value } => {
                if let Some(ResponseItem::FunctionCall { arguments, .. }) =
                    input.get_mut(item_index)
                {
                    *arguments = value;
                }
            }
            PlanProjectionRollback::Output { item_index, body } => {
                if let Some(
                    ResponseItem::FunctionCallOutput { output, .. }
                    | ResponseItem::CustomToolCallOutput { output, .. },
                ) = input.get_mut(item_index)
                {
                    output.body = body;
                }
            }
        }
    }
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
    let mut counter = SerializedByteCounter::default();
    if serde_json::to_writer(&mut counter, value).is_err() {
        return 0;
    }
    i64::try_from(approx_tokens_from_byte_count(counter.bytes)).unwrap_or(0)
}

#[derive(Default)]
struct SerializedByteCounter {
    bytes: usize,
}

impl Write for SerializedByteCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_plan_call(call_id: &str, step: &str) -> ResponseItem {
        update_plan_call_with_explanation(call_id, step, None)
    }

    fn update_plan_call_with_explanation(
        call_id: &str,
        step: &str,
        explanation: Option<&str>,
    ) -> ResponseItem {
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "update_plan".to_string(),
            namespace: None,
            arguments: serde_json::json!({
                "explanation": explanation,
                "plan": [{ "step": step, "status": "in_progress" }],
            })
            .to_string(),
            call_id: call_id.to_string(),
        }
    }

    fn tool_output(call_id: &str, text: String) -> ResponseItem {
        ResponseItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output: FunctionCallOutputBody::Text(text).into(),
        }
    }

    #[test]
    fn plan_working_state_uses_developer_role() {
        let repeated_step =
            "Inspect the complete provider request boundary and preserve the shared \
            state projection without emitting unsupported message roles. "
                .repeat(32);
        let mut input = vec![
            update_plan_call("plan-1", &repeated_step),
            update_plan_call("plan-2", &repeated_step),
        ];

        assert!(project_plan_working_state(&mut input).is_some());

        let working_state_roles = input
            .iter()
            .filter_map(|item| match item {
                ResponseItem::Message { role, content, .. }
                    if content.iter().any(|item| {
                        matches!(
                            item,
                            ContentItem::InputText { text }
                                if text.contains("<praxis-working-state>")
                        )
                    }) =>
                {
                    Some(role.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(working_state_roles, vec!["developer"]);
        assert!(
            !input
                .iter()
                .any(|item| matches!(item, ResponseItem::Message { role, .. } if role == "system"))
        );
    }

    #[test]
    fn plan_working_state_keeps_only_the_latest_complete_snapshot() {
        let old_step = "old step ".repeat(128);
        let latest_step = "latest step ".repeat(128);
        let mut input = vec![
            update_plan_call_with_explanation("plan-1", &old_step, Some("old explanation")),
            update_plan_call_with_explanation("plan-2", &latest_step, Some("latest explanation")),
        ];

        assert!(project_plan_working_state(&mut input).is_some());

        let state = input
            .iter()
            .find_map(|item| match item {
                ResponseItem::Message { content, .. } => {
                    content.iter().find_map(|item| match item {
                        ContentItem::InputText { text }
                            if text.contains("<praxis-working-state>") =>
                        {
                            Some(text)
                        }
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("working state");

        assert!(state.contains("praxis.plan-state.v2"));
        assert!(state.contains("plan-2"));
        assert!(state.contains("latest explanation"));
        assert!(state.contains(&latest_step));
        assert!(!state.contains("plan-1"));
        assert!(!state.contains("old explanation"));
        assert!(!state.contains(&old_step));
    }

    #[test]
    fn repeated_tool_outputs_project_append_only_growth_as_a_delta() {
        let prefix = "unchanged build output\n".repeat(128);
        let suffix = "new compiler error\n".repeat(8);
        let mut input = vec![
            tool_output("call-1", prefix.clone()),
            tool_output("call-2", format!("{prefix}{suffix}")),
        ];

        let events = project_repeated_tool_outputs(&mut input);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TokenSavingKind::OutputDelta);
        let ResponseItem::FunctionCallOutput { output, .. } = &input[1] else {
            panic!("expected function output");
        };
        let FunctionCallOutputBody::Text(projected) = &output.body else {
            panic!("expected text output");
        };
        assert!(projected.contains("source_call_id=\"call-1\""));
        assert!(projected.contains(&suffix));
        assert!(!projected.contains(&prefix));
    }
}
