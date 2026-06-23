use super::*;

#[test]
fn remove_first_item_removes_matching_output_for_function_call() {
    let items = vec![
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "do_it".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-1".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        },
    ];
    let mut h = create_history_with_items(items);
    h.remove_first_item();
    assert_eq!(h.raw_items(), vec![]);
}

#[test]
fn remove_first_item_removes_matching_call_for_output() {
    let items = vec![
        ResponseItem::FunctionCallOutput {
            call_id: "call-2".to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        },
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "do_it".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-2".to_string(),
        },
    ];
    let mut h = create_history_with_items(items);
    h.remove_first_item();
    assert_eq!(h.raw_items(), vec![]);
}

#[test]
fn remove_last_item_removes_matching_call_for_output() {
    let items = vec![
        user_msg("before tool call"),
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "do_it".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-delete-last".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-delete-last".to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        },
    ];
    let mut h = create_history_with_items(items);

    assert!(h.remove_last_item());
    assert_eq!(h.raw_items(), vec![user_msg("before tool call")]);
}

#[test]
fn replace_last_turn_images_replaces_tool_output_images() {
    let items = vec![
        user_input_text_msg("hi"),
        ResponseItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::ContentItems(vec![
                    FunctionCallOutputContentItem::InputImage {
                        image_url: "data:image/png;base64,AAA".to_string(),
                        detail: None,
                    },
                ]),
                success: Some(true),
            },
        },
    ];
    let mut history = create_history_with_items(items);

    assert!(history.replace_last_turn_images("Invalid image"));

    assert_eq!(
        history.raw_items(),
        vec![
            user_input_text_msg("hi"),
            ResponseItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: FunctionCallOutputPayload {
                    body: FunctionCallOutputBody::ContentItems(vec![
                        FunctionCallOutputContentItem::InputText {
                            text: "Invalid image".to_string(),
                        },
                    ]),
                    success: Some(true),
                },
            },
        ]
    );
}

#[test]
fn replace_last_turn_images_does_not_touch_user_images() {
    let items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "data:image/png;base64,AAA".to_string(),
        }],
        end_turn: None,
        phase: None,
    }];
    let mut history = create_history_with_items(items.clone());

    assert!(!history.replace_last_turn_images("Invalid image"));
    assert_eq!(history.raw_items(), items);
}

#[test]
fn remove_first_item_handles_local_shell_pair() {
    let items = vec![
        ResponseItem::LocalShellCall {
            id: None,
            call_id: Some("call-3".to_string()),
            status: LocalShellStatus::Completed,
            action: LocalShellAction::Exec(LocalShellExecAction {
                command: vec!["echo".to_string(), "hi".to_string()],
                timeout_ms: None,
                working_directory: None,
                env: None,
                user: None,
            }),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "call-3".to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        },
    ];
    let mut h = create_history_with_items(items);
    h.remove_first_item();
    assert_eq!(h.raw_items(), vec![]);
}

#[test]
fn drop_last_n_user_turns_preserves_prefix() {
    let items = vec![
        assistant_msg("session prefix item"),
        user_msg("u1"),
        assistant_msg("a1"),
        user_msg("u2"),
        assistant_msg("a2"),
    ];

    let modalities = default_input_modalities();
    let mut history = create_history_with_items(items);
    history.drop_last_n_user_turns(/*num_turns*/ 1);
    assert_eq!(
        history.for_prompt(&modalities),
        vec![
            assistant_msg("session prefix item"),
            user_msg("u1"),
            assistant_msg("a1"),
        ]
    );

    let mut history = create_history_with_items(vec![
        assistant_msg("session prefix item"),
        user_msg("u1"),
        assistant_msg("a1"),
        user_msg("u2"),
        assistant_msg("a2"),
    ]);
    history.drop_last_n_user_turns(/*num_turns*/ 99);
    assert_eq!(
        history.for_prompt(&modalities),
        vec![assistant_msg("session prefix item")]
    );
}

#[test]
fn drop_last_n_user_turns_ignores_session_prefix_user_messages() {
    let items = vec![
        user_input_text_msg("<environment_context>ctx</environment_context>"),
        user_input_text_msg(
            "# AGENTS.md instructions for test_directory\n\n<INSTRUCTIONS>\ntest_text\n</INSTRUCTIONS>",
        ),
        user_input_text_msg(
            "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>",
        ),
        user_input_text_msg("<user_shell_command>echo 42</user_shell_command>"),
        user_input_text_msg(
            "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>",
        ),
        user_input_text_msg("turn 1 user"),
        assistant_msg("turn 1 assistant"),
        user_input_text_msg("turn 2 user"),
        assistant_msg("turn 2 assistant"),
    ];

    let modalities = default_input_modalities();
    let mut history = create_history_with_items(items);
    history.drop_last_n_user_turns(/*num_turns*/ 1);

    let expected_prefix_and_first_turn = vec![
        user_input_text_msg("<environment_context>ctx</environment_context>"),
        user_input_text_msg(
            "# AGENTS.md instructions for test_directory\n\n<INSTRUCTIONS>\ntest_text\n</INSTRUCTIONS>",
        ),
        user_input_text_msg(
            "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>",
        ),
        user_input_text_msg("<user_shell_command>echo 42</user_shell_command>"),
        user_input_text_msg(
            "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>",
        ),
        user_input_text_msg("turn 1 user"),
        assistant_msg("turn 1 assistant"),
    ];

    assert_eq!(
        history.for_prompt(&modalities),
        expected_prefix_and_first_turn
    );

    let expected_prefix_only = vec![
        user_input_text_msg("<environment_context>ctx</environment_context>"),
        user_input_text_msg(
            "# AGENTS.md instructions for test_directory\n\n<INSTRUCTIONS>\ntest_text\n</INSTRUCTIONS>",
        ),
        user_input_text_msg(
            "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>",
        ),
        user_input_text_msg("<user_shell_command>echo 42</user_shell_command>"),
        user_input_text_msg(
            "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>",
        ),
    ];

    let mut history = create_history_with_items(vec![
        user_input_text_msg("<environment_context>ctx</environment_context>"),
        user_input_text_msg(
            "# AGENTS.md instructions for test_directory\n\n<INSTRUCTIONS>\ntest_text\n</INSTRUCTIONS>",
        ),
        user_input_text_msg(
            "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>",
        ),
        user_input_text_msg("<user_shell_command>echo 42</user_shell_command>"),
        user_input_text_msg(
            "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>",
        ),
        user_input_text_msg("turn 1 user"),
        assistant_msg("turn 1 assistant"),
        user_input_text_msg("turn 2 user"),
        assistant_msg("turn 2 assistant"),
    ]);
    history.drop_last_n_user_turns(/*num_turns*/ 2);
    assert_eq!(history.for_prompt(&modalities), expected_prefix_only);

    let mut history = create_history_with_items(vec![
        user_input_text_msg("<environment_context>ctx</environment_context>"),
        user_input_text_msg(
            "# AGENTS.md instructions for test_directory\n\n<INSTRUCTIONS>\ntest_text\n</INSTRUCTIONS>",
        ),
        user_input_text_msg(
            "<skill>\n<name>demo</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>",
        ),
        user_input_text_msg("<user_shell_command>echo 42</user_shell_command>"),
        user_input_text_msg(
            "<subagent_notification>{\"agent_id\":\"a\",\"status\":\"completed\"}</subagent_notification>",
        ),
        user_input_text_msg("turn 1 user"),
        assistant_msg("turn 1 assistant"),
        user_input_text_msg("turn 2 user"),
        assistant_msg("turn 2 assistant"),
    ]);
    history.drop_last_n_user_turns(/*num_turns*/ 3);
    assert_eq!(history.for_prompt(&modalities), expected_prefix_only);
}

#[test]
fn drop_last_n_user_turns_trims_context_updates_above_rolled_back_turn() {
    let items = vec![
        assistant_msg("session prefix item"),
        user_input_text_msg("turn 1 user"),
        assistant_msg("turn 1 assistant"),
        developer_msg("Generated images are saved to /tmp as /tmp/image-1.png by default."),
        developer_msg("<collaboration_mode>ROLLED_BACK_DEV_INSTRUCTIONS</collaboration_mode>"),
        user_input_text_msg(
            "<environment_context><cwd>PRETURN_CONTEXT_DIFF_CWD</cwd></environment_context>",
        ),
        user_input_text_msg("turn 2 user"),
        assistant_msg("turn 2 assistant"),
    ];

    let modalities = default_input_modalities();
    let mut history = create_history_with_items(items);
    let reference_context_item = reference_context_item();
    history.set_reference_context_item(Some(reference_context_item.clone()));
    history.drop_last_n_user_turns(/*num_turns*/ 1);

    assert_eq!(
        history.clone().for_prompt(&modalities),
        vec![
            assistant_msg("session prefix item"),
            user_input_text_msg("turn 1 user"),
            assistant_msg("turn 1 assistant"),
            developer_msg("Generated images are saved to /tmp as /tmp/image-1.png by default."),
        ]
    );
    assert_eq!(
        serde_json::to_value(history.reference_context_item())
            .expect("serialize retained reference context item"),
        serde_json::to_value(Some(reference_context_item))
            .expect("serialize expected reference context item")
    );
}

#[test]
fn drop_last_n_user_turns_clears_reference_context_for_mixed_developer_context_bundles() {
    let items = vec![
        user_input_text_msg("turn 1 user"),
        assistant_msg("turn 1 assistant"),
        developer_msg_with_fragments(&[
            "<permissions instructions>contextual permissions</permissions instructions>",
            "persistent plugin instructions",
        ]),
        user_input_text_msg(
            "<environment_context><cwd>PRETURN_CONTEXT_DIFF_CWD</cwd></environment_context>",
        ),
        user_input_text_msg("turn 2 user"),
        assistant_msg("turn 2 assistant"),
    ];

    let modalities = default_input_modalities();
    let mut history = create_history_with_items(items);
    history.set_reference_context_item(Some(reference_context_item()));
    history.drop_last_n_user_turns(/*num_turns*/ 1);

    assert_eq!(
        history.clone().for_prompt(&modalities),
        vec![
            user_input_text_msg("turn 1 user"),
            assistant_msg("turn 1 assistant"),
        ]
    );
    assert!(history.reference_context_item().is_none());
}
