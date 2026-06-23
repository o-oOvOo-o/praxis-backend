use super::*;

#[test]
fn for_prompt_strips_images_when_model_does_not_support_images() {
    let items = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "look at this".to_string(),
                },
                ContentItem::InputImage {
                    image_url: "https://example.com/img.png".to_string(),
                },
                ContentItem::InputText {
                    text: "caption".to_string(),
                },
            ],
            end_turn: None,
            phase: None,
        },
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "view_image".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-1".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "image result".to_string(),
                },
                FunctionCallOutputContentItem::InputImage {
                    image_url: "https://example.com/result.png".to_string(),
                    detail: None,
                },
            ]),
        },
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "tool-1".to_string(),
            name: "js_repl".to_string(),
            input: "view_image".to_string(),
        },
        ResponseItem::CustomToolCallOutput {
            call_id: "tool-1".to_string(),
            name: None,
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "js repl result".to_string(),
                },
                FunctionCallOutputContentItem::InputImage {
                    image_url: "https://example.com/js-repl-result.png".to_string(),
                    detail: None,
                },
            ]),
        },
    ];
    let history = create_history_with_items(items);
    let text_only_modalities = vec![InputModality::Text];
    let stripped = history.for_prompt(&text_only_modalities);

    let expected = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "look at this".to_string(),
                },
                ContentItem::InputText {
                    text: "image content omitted because you do not support image input"
                        .to_string(),
                },
                ContentItem::InputText {
                    text: "caption".to_string(),
                },
            ],
            end_turn: None,
            phase: None,
        },
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "view_image".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-1".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "image result".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "image content omitted because you do not support image input"
                        .to_string(),
                },
            ]),
        },
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "tool-1".to_string(),
            name: "js_repl".to_string(),
            input: "view_image".to_string(),
        },
        ResponseItem::CustomToolCallOutput {
            call_id: "tool-1".to_string(),
            name: None,
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "js repl result".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "image content omitted because you do not support image input"
                        .to_string(),
                },
            ]),
        },
    ];
    assert_eq!(stripped, expected);

    // With image support, images are preserved
    let modalities = default_input_modalities();
    let with_images = create_history_with_items(vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "look".to_string(),
            },
            ContentItem::InputImage {
                image_url: "https://example.com/img.png".to_string(),
            },
        ],
        end_turn: None,
        phase: None,
    }]);
    let preserved = with_images.for_prompt(&modalities);
    assert_eq!(preserved.len(), 1);
    if let ResponseItem::Message { content, .. } = &preserved[0] {
        assert_eq!(content.len(), 2);
        assert!(matches!(content[1], ContentItem::InputImage { .. }));
    } else {
        panic!("expected Message");
    }
}

#[test]
fn for_prompt_preserves_image_generation_calls_when_images_are_supported() {
    let history = create_history_with_items(vec![
        ResponseItem::ImageGenerationCall {
            id: "ig_123".to_string(),
            status: "generating".to_string(),
            revised_prompt: Some("lobster".to_string()),
            result: "Zm9v".to_string(),
        },
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hi".to_string(),
            }],
            end_turn: None,
            phase: None,
        },
    ]);

    assert_eq!(
        history.for_prompt(&default_input_modalities()),
        vec![
            ResponseItem::ImageGenerationCall {
                id: "ig_123".to_string(),
                status: "generating".to_string(),
                revised_prompt: Some("lobster".to_string()),
                result: "Zm9v".to_string(),
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hi".to_string(),
                }],
                end_turn: None,
                phase: None,
            }
        ]
    );
}

#[test]
fn for_prompt_clears_image_generation_result_when_images_are_unsupported() {
    let history = create_history_with_items(vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "generate a lobster".to_string(),
            }],
            end_turn: None,
            phase: None,
        },
        ResponseItem::ImageGenerationCall {
            id: "ig_123".to_string(),
            status: "completed".to_string(),
            revised_prompt: Some("lobster".to_string()),
            result: "Zm9v".to_string(),
        },
    ]);

    assert_eq!(
        history.for_prompt(&[InputModality::Text]),
        vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "generate a lobster".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::ImageGenerationCall {
                id: "ig_123".to_string(),
                status: "completed".to_string(),
                revised_prompt: Some("lobster".to_string()),
                result: String::new(),
            },
        ]
    );
}

#[test]
fn get_history_for_prompt_drops_ghost_commits() {
    let items = vec![ResponseItem::GhostSnapshot {
        ghost_commit: GhostCommit::new(
            "ghost-1".to_string(),
            /*parent*/ None,
            Vec::new(),
            Vec::new(),
        ),
    }];
    let history = create_history_with_items(items);
    let modalities = default_input_modalities();
    let filtered = history.for_prompt(&modalities);
    assert_eq!(filtered, vec![]);
}

#[test]
fn estimate_token_count_with_base_instructions_uses_provided_text() {
    let history = create_history_with_items(vec![assistant_msg("hello from history")]);
    let short_base = BaseInstructions {
        text: "short".to_string(),
    };
    let long_base = BaseInstructions {
        text: "x".repeat(1_000),
    };

    let short_estimate = history
        .estimate_token_count_with_base_instructions(&short_base)
        .expect("token estimate");
    let long_estimate = history
        .estimate_token_count_with_base_instructions(&long_base)
        .expect("token estimate");

    let expected_delta = approx_token_count_for_text(&long_base.text)
        - approx_token_count_for_text(&short_base.text);
    assert_eq!(long_estimate - short_estimate, expected_delta);
}
