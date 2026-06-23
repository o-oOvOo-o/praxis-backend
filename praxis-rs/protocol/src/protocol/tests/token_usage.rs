use super::*;

#[test]
fn token_usage_info_new_or_append_updates_context_window_when_provided() {
    let initial = Some(TokenUsageInfo {
        total_token_usage: TokenUsage::default(),
        last_token_usage: TokenUsage::default(),
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
