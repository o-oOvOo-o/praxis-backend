use super::*;

#[test]
fn token_usage_info_new_or_append_updates_context_window_when_provided() {
    let initial = Some(TokenUsageInfo {
        total_token_usage: TokenUsage::default(),
        last_token_usage: TokenUsage::default(),
        internal_savings: Default::default(),
        model_context_window: Some(258_400),
        model_auto_compact_token_limit: None,
    });
    let last = Some(TokenUsage {
        input_tokens: 10,
        cached_input_tokens: 0,
        cache_reported_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 10,
    });

    let info = TokenUsageInfo::new_or_append(&initial, &last, Some(128_000), Some(120_000))
        .expect("new_or_append should return info");

    assert_eq!(info.model_context_window, Some(128_000));
    assert_eq!(info.model_auto_compact_token_limit, Some(120_000));
}

#[test]
fn token_usage_info_new_or_append_preserves_context_window_when_not_provided() {
    let initial = Some(TokenUsageInfo {
        total_token_usage: TokenUsage::default(),
        last_token_usage: TokenUsage::default(),
        internal_savings: Default::default(),
        model_context_window: Some(258_400),
        model_auto_compact_token_limit: Some(244_000),
    });
    let last = Some(TokenUsage {
        input_tokens: 10,
        cached_input_tokens: 0,
        cache_reported_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: 10,
    });

    let info = TokenUsageInfo::new_or_append(
        &initial, &last, /*model_context_window*/ None,
        /*model_auto_compact_token_limit*/ None,
    )
    .expect("new_or_append should return info");

    assert_eq!(info.model_context_window, Some(258_400));
    assert_eq!(info.model_auto_compact_token_limit, Some(244_000));
}

#[test]
fn internal_savings_are_thread_local_and_survive_provider_usage_updates() {
    let mut initial = TokenUsageInfo {
        total_token_usage: TokenUsage::default(),
        last_token_usage: TokenUsage::default(),
        internal_savings: Default::default(),
        model_context_window: Some(128_000),
        model_auto_compact_token_limit: None,
    };
    initial.internal_savings.record(1_200);
    initial.internal_savings.record(300);

    let info = TokenUsageInfo::new_or_append(
        &Some(initial),
        &Some(TokenUsage {
            total_tokens: 42,
            ..TokenUsage::default()
        }),
        None,
        None,
    )
    .expect("usage update should preserve the thread ledger");

    assert_eq!(info.internal_savings.total_saved_tokens, 1_500);
    assert_eq!(info.internal_savings.last_saved_tokens, 300);
}

#[test]
fn token_savings_only_record_reversible_events() {
    let mut savings = TokenSavingsInfo::default();
    savings.record_event(TokenSavingEvent::new(
        TokenSavingKind::UnchangedResource,
        800,
        100,
        false,
        None,
    ));

    assert_eq!(savings, TokenSavingsInfo::default());
}

#[test]
fn token_savings_accumulate_by_category() {
    let mut savings = TokenSavingsInfo::default();
    savings.record_event(TokenSavingEvent::new(
        TokenSavingKind::ToolSchemaElision,
        900,
        100,
        true,
        Some("dynamic-tools://deferred".to_string()),
    ));
    savings.record_event(TokenSavingEvent::new(
        TokenSavingKind::ToolSchemaElision,
        500,
        100,
        true,
        Some("dynamic-tools://deferred".to_string()),
    ));

    assert_eq!(savings.total_saved_tokens, 1_200);
    assert_eq!(savings.last_saved_tokens, 400);
    assert_eq!(savings.categories.len(), 1);
    assert_eq!(
        savings.categories[0].kind,
        TokenSavingKind::ToolSchemaElision
    );
    assert_eq!(savings.categories[0].total_saved_tokens, 1_200);
    assert_eq!(savings.categories[0].occurrences, 2);
}
