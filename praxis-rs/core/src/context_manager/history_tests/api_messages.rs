use super::*;

#[test]
fn filters_non_api_messages() {
    let mut h = ContextManager::default();
    let policy = TruncationPolicy::Tokens(10_000);
    // System message is not API messages; Other is ignored.
    let system = ResponseItem::Message {
        id: None,
        role: "system".to_string(),
        content: vec![ContentItem::OutputText {
            text: "ignored".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    let reasoning = reasoning_msg("thinking...");
    h.record_items([&system, &reasoning, &ResponseItem::Other], policy);

    // User and assistant should be retained.
    let u = user_msg("hi");
    let a = assistant_msg("hello");
    h.record_items([&u, &a], policy);

    let items = h.raw_items();
    assert_eq!(
        items,
        vec![
            ResponseItem::Reasoning {
                id: String::new(),
                summary: vec![ReasoningItemReasoningSummary::SummaryText {
                    text: "summary".to_string(),
                }],
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: "thinking...".to_string(),
                }]),
                encrypted_content: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "hi".to_string()
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "hello".to_string()
                }],
                end_turn: None,
                phase: None,
            }
        ]
    );
}

#[test]
fn non_last_reasoning_tokens_return_zero_when_no_user_messages() {
    let history =
        create_history_with_items(vec![reasoning_with_encrypted_content(/*len*/ 800)]);

    assert_eq!(history.get_non_last_reasoning_items_tokens(), 0);
}

#[test]
fn non_last_reasoning_tokens_ignore_entries_after_last_user() {
    let history = create_history_with_items(vec![
        reasoning_with_encrypted_content(/*len*/ 900),
        user_msg("first"),
        reasoning_with_encrypted_content(/*len*/ 1_000),
        user_msg("second"),
        reasoning_with_encrypted_content(/*len*/ 2_000),
    ]);
    // first: (900 * 0.75 - 650) / 4 = 6.25 tokens
    // second: (1000 * 0.75 - 650) / 4 = 25 tokens
    // first + second = 62.5
    assert_eq!(history.get_non_last_reasoning_items_tokens(), 32);
}

#[test]
fn items_after_last_model_generated_tokens_include_user_and_tool_output() {
    let history = create_history_with_items(vec![
        assistant_msg("already counted by API"),
        user_msg("new user message"),
        custom_tool_call_output("call-tail", "new tool output"),
    ]);
    let expected_tokens = estimate_item_token_count(&user_msg("new user message")).saturating_add(
        estimate_item_token_count(&custom_tool_call_output("call-tail", "new tool output")),
    );

    assert_eq!(
        history
            .items_after_last_model_generated_item()
            .iter()
            .map(estimate_item_token_count)
            .fold(0i64, i64::saturating_add),
        expected_tokens
    );
}

#[test]
fn items_after_last_model_generated_tokens_are_zero_without_model_generated_items() {
    let history = create_history_with_items(vec![user_msg("no model output yet")]);

    assert_eq!(
        history
            .items_after_last_model_generated_item()
            .iter()
            .map(estimate_item_token_count)
            .fold(0i64, i64::saturating_add),
        0
    );
}

#[test]
fn inter_agent_assistant_messages_are_turn_boundaries() {
    let item = inter_agent_assistant_msg("continue");

    assert!(is_user_turn_boundary(&item));
}

#[test]
fn for_prompt_preserves_inter_agent_assistant_messages() {
    let item = inter_agent_assistant_msg("continue");
    let history = create_history_with_items(vec![item.clone()]);

    assert_eq!(history.raw_items(), std::slice::from_ref(&item));
    assert_eq!(history.for_prompt(&default_input_modalities()), vec![item]);
}

#[test]
fn drop_last_n_user_turns_treats_inter_agent_assistant_messages_as_instruction_turns() {
    let first_turn = user_input_text_msg("first");
    let first_reply = assistant_msg("done");
    let inter_agent_turn = inter_agent_assistant_msg("continue");
    let inter_agent_reply = assistant_msg("worker reply");
    let mut history = create_history_with_items(vec![
        first_turn.clone(),
        first_reply.clone(),
        inter_agent_turn,
        inter_agent_reply,
    ]);

    history.drop_last_n_user_turns(/*num_turns*/ 1);

    assert_eq!(history.raw_items(), &vec![first_turn, first_reply]);
}

#[test]
fn legacy_inter_agent_assistant_messages_are_not_turn_boundaries() {
    let item = assistant_msg(
        "author: /root\nrecipient: /root/worker\nother_recipients: []\nContent: continue",
    );

    assert!(!is_user_turn_boundary(&item));
}

#[test]
fn total_token_usage_includes_all_items_after_last_model_generated_item() {
    let mut history = create_history_with_items(vec![assistant_msg("already counted by API")]);
    history.update_token_info(
        &TokenUsage {
            total_tokens: 100,
            ..Default::default()
        },
        /*model_context_window*/ None,
        /*model_auto_compact_token_limit*/ None,
    );
    let added_user = user_msg("new user message");
    let added_tool_output = custom_tool_call_output("tool-tail", "new tool output");
    history.record_items(
        [&added_user, &added_tool_output],
        TruncationPolicy::Tokens(10_000),
    );

    assert_eq!(
        history.get_total_token_usage(/*server_reasoning_included*/ true),
        100 + estimate_item_token_count(&added_user)
            + estimate_item_token_count(&added_tool_output)
    );
}
