use super::*;
use crate::config_types::SandboxMode;
use crate::protocol::AskForApproval;
use crate::protocol::GranularApprovalConfig;
use anyhow::Result;
use praxis_execpolicy::Policy;
use pretty_assertions::assert_eq;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn sandbox_permissions_helpers_match_documented_semantics() {
    let cases = [
        (SandboxPermissions::UseDefault, false, false, false),
        (SandboxPermissions::RequireEscalated, true, true, false),
        (
            SandboxPermissions::WithAdditionalPermissions,
            false,
            true,
            true,
        ),
    ];

    for (
        sandbox_permissions,
        requires_escalated_permissions,
        requests_sandbox_override,
        uses_additional_permissions,
    ) in cases
    {
        assert_eq!(
            sandbox_permissions.requires_escalated_permissions(),
            requires_escalated_permissions
        );
        assert_eq!(
            sandbox_permissions.requests_sandbox_override(),
            requests_sandbox_override
        );
        assert_eq!(
            sandbox_permissions.uses_additional_permissions(),
            uses_additional_permissions
        );
    }
}

#[test]
fn shell_tool_params_accept_command_argv_array() {
    let params: ShellToolCallParams =
        serde_json::from_str(r#"{"command":["powershell.exe","-Command","pwd"]}"#)
            .expect("argv array should parse");

    assert_eq!(params.command, vec!["powershell.exe", "-Command", "pwd"]);
}

#[test]
fn shell_tool_params_accept_json_encoded_command_argv_string() {
    let params: ShellToolCallParams =
        serde_json::from_str(r#"{"command":"[\"powershell.exe\",\"-Command\",\"pwd\"]"}"#)
            .expect("stringified argv should parse for provider compatibility");

    assert_eq!(params.command, vec!["powershell.exe", "-Command", "pwd"]);
}

#[test]
fn shell_tool_params_reject_plain_command_string() {
    let err = serde_json::from_str::<ShellToolCallParams>(r#"{"command":"powershell pwd"}"#)
        .expect_err("plain shell strings belong to shell_command, not shell argv");

    assert!(
        err.to_string().contains("argv array"),
        "unexpected error: {err}"
    );
}

#[test]
fn convert_mcp_content_to_items_preserves_data_urls() {
    let contents = vec![serde_json::json!({
        "type": "image",
        "data": "data:image/png;base64,Zm9v",
        "mimeType": "image/png",
    })];

    let items = convert_mcp_content_to_items(&contents).expect("expected image items");
    assert_eq!(
        items,
        vec![FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,Zm9v".to_string(),
            detail: None,
        }]
    );
}

#[test]
fn response_item_parses_image_generation_call() {
    let item = serde_json::from_value::<ResponseItem>(serde_json::json!({
        "id": "ig_123",
        "type": "image_generation_call",
        "status": "completed",
        "revised_prompt": "A small blue square",
        "result": "Zm9v",
    }))
    .expect("image generation item should deserialize");

    assert_eq!(
        item,
        ResponseItem::ImageGenerationCall {
            id: "ig_123".to_string(),
            status: "completed".to_string(),
            revised_prompt: Some("A small blue square".to_string()),
            result: "Zm9v".to_string(),
        }
    );
}

#[test]
fn response_item_parses_image_generation_call_without_revised_prompt() {
    let item = serde_json::from_value::<ResponseItem>(serde_json::json!({
        "id": "ig_123",
        "type": "image_generation_call",
        "status": "completed",
        "result": "Zm9v",
    }))
    .expect("image generation item should deserialize");

    assert_eq!(
        item,
        ResponseItem::ImageGenerationCall {
            id: "ig_123".to_string(),
            status: "completed".to_string(),
            revised_prompt: None,
            result: "Zm9v".to_string(),
        }
    );
}

#[test]
fn permission_profile_is_empty_when_all_fields_are_none() {
    assert_eq!(PermissionProfile::default().is_empty(), true);
}

#[test]
fn permission_profile_is_not_empty_when_field_is_present_but_nested_empty() {
    let permission_profile = PermissionProfile {
        network: Some(NetworkPermissions { enabled: None }),
        file_system: None,
    };
    assert_eq!(permission_profile.is_empty(), false);
}

#[test]
fn convert_mcp_content_to_items_builds_data_urls_when_missing_prefix() {
    let contents = vec![serde_json::json!({
        "type": "image",
        "data": "Zm9v",
        "mimeType": "image/png",
    })];

    let items = convert_mcp_content_to_items(&contents).expect("expected image items");
    assert_eq!(
        items,
        vec![FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,Zm9v".to_string(),
            detail: None,
        }]
    );
}

#[test]
fn convert_mcp_content_to_items_returns_none_without_images() {
    let contents = vec![serde_json::json!({
        "type": "text",
        "text": "hello",
    })];

    assert_eq!(convert_mcp_content_to_items(&contents), None);
}

#[test]
fn function_call_output_content_items_to_text_joins_text_segments() {
    let content_items = vec![
        FunctionCallOutputContentItem::InputText {
            text: "line 1".to_string(),
        },
        FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,AAA".to_string(),
            detail: None,
        },
        FunctionCallOutputContentItem::InputText {
            text: "line 2".to_string(),
        },
    ];

    let text = function_call_output_content_items_to_text(&content_items);
    assert_eq!(text, Some("line 1\nline 2".to_string()));
}

#[test]
fn function_call_output_content_items_to_text_ignores_blank_text_and_images() {
    let content_items = vec![
        FunctionCallOutputContentItem::InputText {
            text: "   ".to_string(),
        },
        FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,AAA".to_string(),
            detail: None,
        },
    ];

    let text = function_call_output_content_items_to_text(&content_items);
    assert_eq!(text, None);
}

#[test]
fn function_call_output_body_to_text_returns_plain_text_content() {
    let body = FunctionCallOutputBody::Text("ok".to_string());
    let text = body.to_text();
    assert_eq!(text, Some("ok".to_string()));
}

#[test]
fn function_call_output_body_to_text_uses_content_item_fallback() {
    let body = FunctionCallOutputBody::ContentItems(vec![
        FunctionCallOutputContentItem::InputText {
            text: "line 1".to_string(),
        },
        FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,AAA".to_string(),
            detail: None,
        },
    ]);

    let text = body.to_text();
    assert_eq!(text, Some("line 1".to_string()));
}

#[test]
fn function_call_deserializes_optional_namespace() {
    let item: ResponseItem = serde_json::from_value(serde_json::json!({
        "type": "function_call",
        "name": "mcp__praxis_apps__gmail_get_recent_emails",
        "namespace": "mcp__praxis_apps__gmail",
        "arguments": "{\"top_k\":5}",
        "call_id": "call-1",
    }))
    .expect("function_call should deserialize");

    assert_eq!(
        item,
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "mcp__praxis_apps__gmail_get_recent_emails".to_string(),
            namespace: Some("mcp__praxis_apps__gmail".to_string()),
            arguments: "{\"top_k\":5}".to_string(),
            call_id: "call-1".to_string(),
        }
    );
}

#[test]
fn converts_sandbox_mode_into_developer_instructions() {
    let workspace_write: DeveloperInstructions = SandboxMode::WorkspaceWrite.into();
    assert_eq!(
        workspace_write,
        DeveloperInstructions::new(
            "Filesystem sandboxing defines which files can be read or written. `sandbox_mode` is `workspace-write`: The sandbox permits reading files, and editing files in `cwd` and `writable_roots`. Editing files in other directories requires approval. Network access is restricted."
        )
    );

    let read_only: DeveloperInstructions = SandboxMode::ReadOnly.into();
    assert_eq!(
        read_only,
        DeveloperInstructions::new(
            "Filesystem sandboxing defines which files can be read or written. `sandbox_mode` is `read-only`: The sandbox only permits reading files. Network access is restricted."
        )
    );

    let danger_full_access: DeveloperInstructions = SandboxMode::DangerFullAccess.into();
    assert_eq!(
        danger_full_access,
        DeveloperInstructions::new(
            "Filesystem sandboxing defines which files can be read or written. `sandbox_mode` is `danger-full-access`: No filesystem sandboxing - all commands are permitted. Network access is enabled."
        )
    );
}

#[test]
fn builds_permissions_with_network_access_override() {
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &Policy::empty(),
            exec_permission_approvals_enabled: false,
            request_permissions_tool_enabled: false,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(
        text.contains("Network access is enabled."),
        "expected network access to be enabled in message"
    );
    assert!(
        text.contains("How to request escalation"),
        "expected approval guidance to be included"
    );
}

#[test]
fn builds_permissions_from_policy() {
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access: true,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    };

    let instructions = DeveloperInstructions::from_policy(
        &policy,
        AskForApproval::UnlessTrusted,
        ApprovalsReviewer::User,
        &Policy::empty(),
        &PathBuf::from("/tmp"),
        /*exec_permission_approvals_enabled*/ false,
        /*request_permissions_tool_enabled*/ false,
    );
    let text = instructions.into_text();
    assert!(text.contains("Network access is enabled."));
    assert!(text.contains("`approval_policy` is `unless-trusted`"));
}

#[test]
fn includes_request_rule_instructions_for_on_request() {
    let mut exec_policy = Policy::empty();
    exec_policy
        .add_prefix_rule(
            &["git".to_string(), "pull".to_string()],
            praxis_execpolicy::Decision::Allow,
        )
        .expect("add rule");
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &exec_policy,
            exec_permission_approvals_enabled: false,
            request_permissions_tool_enabled: false,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(text.contains("prefix_rule"));
    assert!(text.contains("Approved command prefixes"));
    assert!(text.contains(r#"["git", "pull"]"#));
}

#[test]
fn includes_request_permissions_tool_instructions_for_unless_trusted_when_enabled() {
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::UnlessTrusted,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &Policy::empty(),
            exec_permission_approvals_enabled: false,
            request_permissions_tool_enabled: true,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(text.contains("`approval_policy` is `unless-trusted`"));
    assert!(text.contains("# request_permissions Tool"));
}

#[test]
fn includes_request_permissions_tool_instructions_for_on_failure_when_enabled() {
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::OnFailure,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &Policy::empty(),
            exec_permission_approvals_enabled: false,
            request_permissions_tool_enabled: true,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(text.contains("`approval_policy` is `on-failure`"));
    assert!(text.contains("# request_permissions Tool"));
}

#[test]
fn includes_request_permission_rule_instructions_for_on_request_when_enabled() {
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &Policy::empty(),
            exec_permission_approvals_enabled: true,
            request_permissions_tool_enabled: false,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(text.contains("with_additional_permissions"));
    assert!(text.contains("additional_permissions"));
}

#[test]
fn includes_request_permissions_tool_instructions_for_on_request_when_tool_is_enabled() {
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &Policy::empty(),
            exec_permission_approvals_enabled: false,
            request_permissions_tool_enabled: true,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(text.contains("# request_permissions Tool"));
    assert!(text.contains("The built-in `request_permissions` tool is available in this session."));
}

#[test]
fn on_request_includes_tool_guidance_alongside_inline_permission_guidance_when_both_exist() {
    let instructions = DeveloperInstructions::from_permissions_with_network(
        SandboxMode::WorkspaceWrite,
        NetworkAccess::Enabled,
        PermissionsPromptConfig {
            approval_policy: AskForApproval::OnRequest,
            approvals_reviewer: ApprovalsReviewer::User,
            exec_policy: &Policy::empty(),
            exec_permission_approvals_enabled: true,
            request_permissions_tool_enabled: true,
        },
        /*writable_roots*/ None,
    );

    let text = instructions.into_text();
    assert!(text.contains("with_additional_permissions"));
    assert!(text.contains("# request_permissions Tool"));
}

#[test]
fn guardian_subagent_approvals_append_guardian_specific_guidance() {
    let text = DeveloperInstructions::from(
        AskForApproval::OnRequest,
        ApprovalsReviewer::GuardianSubagent,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ false,
        /*request_permissions_tool_enabled*/ false,
    )
    .into_text();

    assert!(text.contains("`approvals_reviewer` is `guardian_subagent`"));
    assert!(text.contains("materially safer alternative"));
}

#[test]
fn guardian_subagent_approvals_omit_guardian_specific_guidance_when_approval_is_never() {
    let text = DeveloperInstructions::from(
        AskForApproval::Never,
        ApprovalsReviewer::GuardianSubagent,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ false,
        /*request_permissions_tool_enabled*/ false,
    )
    .into_text();

    assert!(!text.contains("`approvals_reviewer` is `guardian_subagent`"));
}

fn granular_categories_section(title: &str, categories: &[&str]) -> String {
    format!("{title}\n{}", categories.join("\n"))
}

fn granular_prompt_expected(
    prompted_categories: &[&str],
    rejected_categories: &[&str],
    include_shell_permission_request_instructions: bool,
    include_request_permissions_tool_section: bool,
) -> String {
    let mut sections = vec![granular_prompt_intro_text().to_string()];
    if !prompted_categories.is_empty() {
        sections.push(granular_categories_section(
            "These approval categories may still prompt the user when needed:",
            prompted_categories,
        ));
    }
    if !rejected_categories.is_empty() {
        sections.push(granular_categories_section(
            "These approval categories are automatically rejected instead of prompting the user:",
            rejected_categories,
        ));
    }
    if include_shell_permission_request_instructions {
        sections.push(APPROVAL_POLICY_ON_REQUEST_RULE_REQUEST_PERMISSION.to_string());
    }
    if include_request_permissions_tool_section {
        sections.push(request_permissions_tool_prompt_section().to_string());
    }
    sections.join("\n\n")
}

#[test]
fn granular_policy_lists_prompted_and_rejected_categories_separately() {
    let text = DeveloperInstructions::from(
        AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: false,
            rules: true,
            skill_approval: false,
            request_permissions: true,
            mcp_elicitations: false,
        }),
        ApprovalsReviewer::User,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ true,
        /*request_permissions_tool_enabled*/ false,
    )
    .into_text();

    assert_eq!(
        text,
        [
            granular_prompt_intro_text().to_string(),
            granular_categories_section(
                "These approval categories may still prompt the user when needed:",
                &["- `rules`"],
            ),
            granular_categories_section(
                "These approval categories are automatically rejected instead of prompting the user:",
                &["- `sandbox_approval`", "- `skill_approval`", "- `mcp_elicitations`",],
            ),
        ]
        .join("\n\n")
    );
}

#[test]
fn granular_policy_includes_command_permission_instructions_when_sandbox_approval_can_prompt() {
    let text = DeveloperInstructions::from(
        AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: true,
        }),
        ApprovalsReviewer::User,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ true,
        /*request_permissions_tool_enabled*/ false,
    )
    .into_text();

    assert_eq!(
        text,
        granular_prompt_expected(
            &[
                "- `sandbox_approval`",
                "- `rules`",
                "- `skill_approval`",
                "- `mcp_elicitations`",
            ],
            &[],
            /*include_shell_permission_request_instructions*/ true,
            /*include_request_permissions_tool_section*/ false,
        )
    );
}

#[test]
fn granular_policy_omits_shell_permission_instructions_when_inline_requests_are_disabled() {
    let text = DeveloperInstructions::from(
        AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: true,
        }),
        ApprovalsReviewer::User,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ false,
        /*request_permissions_tool_enabled*/ false,
    )
    .into_text();

    assert_eq!(
        text,
        granular_prompt_expected(
            &[
                "- `sandbox_approval`",
                "- `rules`",
                "- `skill_approval`",
                "- `mcp_elicitations`",
            ],
            &[],
            /*include_shell_permission_request_instructions*/ false,
            /*include_request_permissions_tool_section*/ false,
        )
    );
}

#[test]
fn granular_policy_includes_request_permissions_tool_only_when_that_prompt_can_still_fire() {
    let allowed = DeveloperInstructions::from(
        AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: true,
            mcp_elicitations: true,
        }),
        ApprovalsReviewer::User,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ true,
        /*request_permissions_tool_enabled*/ true,
    )
    .into_text();
    assert!(allowed.contains("# request_permissions Tool"));

    let rejected = DeveloperInstructions::from(
        AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: true,
            rules: true,
            skill_approval: true,
            request_permissions: false,
            mcp_elicitations: true,
        }),
        ApprovalsReviewer::User,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ true,
        /*request_permissions_tool_enabled*/ true,
    )
    .into_text();
    assert!(!rejected.contains("# request_permissions Tool"));
}

#[test]
fn granular_policy_lists_request_permissions_category_without_tool_section_when_tool_is_unavailable()
 {
    let text = DeveloperInstructions::from(
        AskForApproval::Granular(GranularApprovalConfig {
            sandbox_approval: false,
            rules: false,
            skill_approval: false,
            request_permissions: true,
            mcp_elicitations: false,
        }),
        ApprovalsReviewer::User,
        &Policy::empty(),
        /*exec_permission_approvals_enabled*/ true,
        /*request_permissions_tool_enabled*/ false,
    )
    .into_text();

    assert!(!text.contains("- `request_permissions`"));
    assert!(!text.contains("# request_permissions Tool"));
}

#[test]
fn render_command_prefix_list_sorts_by_len_then_total_len_then_alphabetical() {
    let prefixes = vec![
        vec!["b".to_string(), "zz".to_string()],
        vec!["aa".to_string()],
        vec!["b".to_string()],
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        vec!["a".to_string()],
        vec!["b".to_string(), "a".to_string()],
    ];

    let output = format_allow_prefixes(prefixes).expect("rendered list");
    assert_eq!(
        output,
        r#"- ["a"]
- ["b"]
- ["aa"]
- ["b", "a"]
- ["b", "zz"]
- ["a", "b", "c"]"#
            .to_string(),
    );
}

#[test]
fn render_command_prefix_list_limits_output_to_max_prefixes() {
    let prefixes = (0..(MAX_RENDERED_PREFIXES + 5))
        .map(|i| vec![format!("{i:03}")])
        .collect::<Vec<_>>();

    let output = format_allow_prefixes(prefixes).expect("rendered list");
    assert_eq!(output.ends_with(TRUNCATED_MARKER), true);
    eprintln!("output: {output}");
    assert_eq!(output.lines().count(), MAX_RENDERED_PREFIXES + 1);
}

#[test]
fn format_allow_prefixes_limits_output() {
    let mut exec_policy = Policy::empty();
    for i in 0..200 {
        exec_policy
            .add_prefix_rule(
                &[format!("tool-{i:03}"), "x".repeat(500)],
                praxis_execpolicy::Decision::Allow,
            )
            .expect("add rule");
    }

    let output =
        format_allow_prefixes(exec_policy.get_allowed_prefixes()).expect("formatted prefixes");
    assert!(
        output.len() <= MAX_ALLOW_PREFIX_TEXT_BYTES + TRUNCATED_MARKER.len(),
        "output length exceeds expected limit: {output}",
    );
}

#[test]
fn deserializes_compaction_alias() -> Result<()> {
    let json = r#"{"type":"compaction_summary","encrypted_content":"abc"}"#;

    let item: ResponseItem = serde_json::from_str(json)?;

    assert_eq!(
        item,
        ResponseItem::Compaction {
            encrypted_content: "abc".into(),
        }
    );
    Ok(())
}

#[test]
fn roundtrips_web_search_call_actions() -> Result<()> {
    let cases = vec![
        (
            r#"{
                "type": "web_search_call",
                "status": "completed",
                "action": {
                    "type": "search",
                    "query": "weather seattle",
                    "queries": ["weather seattle", "seattle weather now"]
                }
            }"#,
            None,
            Some(WebSearchAction::Search {
                query: Some("weather seattle".into()),
                queries: Some(vec!["weather seattle".into(), "seattle weather now".into()]),
            }),
            Some("completed".into()),
            true,
        ),
        (
            r#"{
                "type": "web_search_call",
                "status": "open",
                "action": {
                    "type": "open_page",
                    "url": "https://example.com"
                }
            }"#,
            None,
            Some(WebSearchAction::OpenPage {
                url: Some("https://example.com".into()),
            }),
            Some("open".into()),
            true,
        ),
        (
            r#"{
                "type": "web_search_call",
                "status": "in_progress",
                "action": {
                    "type": "find_in_page",
                    "url": "https://example.com/docs",
                    "pattern": "installation"
                }
            }"#,
            None,
            Some(WebSearchAction::FindInPage {
                url: Some("https://example.com/docs".into()),
                pattern: Some("installation".into()),
            }),
            Some("in_progress".into()),
            true,
        ),
        (
            r#"{
                "type": "web_search_call",
                "status": "in_progress",
                "id": "ws_partial"
            }"#,
            Some("ws_partial".into()),
            None,
            Some("in_progress".into()),
            false,
        ),
    ];

    for (json_literal, expected_id, expected_action, expected_status, expect_roundtrip) in cases {
        let parsed: ResponseItem = serde_json::from_str(json_literal)?;
        let expected = ResponseItem::WebSearchCall {
            id: expected_id.clone(),
            status: expected_status.clone(),
            action: expected_action.clone(),
        };
        assert_eq!(parsed, expected);

        let serialized = serde_json::to_value(&parsed)?;
        let mut expected_serialized: serde_json::Value = serde_json::from_str(json_literal)?;
        if !expect_roundtrip && let Some(obj) = expected_serialized.as_object_mut() {
            obj.remove("id");
        }
        assert_eq!(serialized, expected_serialized);
    }

    Ok(())
}

#[test]
fn deserialize_shell_tool_call_params() -> Result<()> {
    let json = r#"{
        "command": ["ls", "-l"],
        "workdir": "/tmp",
        "timeout": 1000
    }"#;

    let params: ShellToolCallParams = serde_json::from_str(json)?;
    assert_eq!(
        ShellToolCallParams {
            command: vec!["ls".to_string(), "-l".to_string()],
            workdir: Some("/tmp".to_string()),
            timeout_ms: Some(1000),
            sandbox_permissions: None,
            prefix_rule: None,
            additional_permissions: None,
            justification: None,
        },
        params
    );
    Ok(())
}

#[test]
fn wraps_image_user_input_with_tags() -> Result<()> {
    let image_url = "data:image/png;base64,abc".to_string();

    let item = ResponseInputItem::from(vec![UserInput::Image {
        image_url: image_url.clone(),
    }]);

    match item {
        ResponseInputItem::Message { content, .. } => {
            let expected = vec![
                ContentItem::InputText {
                    text: image_open_tag_text(),
                },
                ContentItem::InputImage { image_url },
                ContentItem::InputText {
                    text: image_close_tag_text(),
                },
            ];
            assert_eq!(content, expected);
        }
        other => panic!("expected message response but got {other:?}"),
    }

    Ok(())
}

#[test]
fn tool_search_call_roundtrips() -> Result<()> {
    let parsed: ResponseItem = serde_json::from_str(
        r#"{
            "type": "tool_search_call",
            "call_id": "search-1",
            "execution": "client",
            "arguments": {
                "query": "calendar create",
                "limit": 1
            }
        }"#,
    )?;

    assert_eq!(
        parsed,
        ResponseItem::ToolSearchCall {
            id: None,
            call_id: Some("search-1".to_string()),
            status: None,
            execution: "client".to_string(),
            arguments: serde_json::json!({
                "query": "calendar create",
                "limit": 1,
            }),
        }
    );

    assert_eq!(
        serde_json::to_value(&parsed)?,
        serde_json::json!({
            "type": "tool_search_call",
            "call_id": "search-1",
            "execution": "client",
            "arguments": {
                "query": "calendar create",
                "limit": 1,
            }
        })
    );

    Ok(())
}

#[test]
fn tool_search_output_roundtrips() -> Result<()> {
    let input = ResponseInputItem::ToolSearchOutput {
        call_id: "search-1".to_string(),
        status: "completed".to_string(),
        execution: "client".to_string(),
        tools: vec![serde_json::json!({
            "type": "function",
            "name": "mcp__praxis_apps__calendar_create_event",
            "description": "Create a calendar event.",
            "defer_loading": true,
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {"type": "string"}
                },
                "required": ["title"],
                "additionalProperties": false,
            }
        })],
    };
    assert_eq!(
        ResponseItem::from(input.clone()),
        ResponseItem::ToolSearchOutput {
            call_id: Some("search-1".to_string()),
            status: "completed".to_string(),
            execution: "client".to_string(),
            tools: vec![serde_json::json!({
                "type": "function",
                "name": "mcp__praxis_apps__calendar_create_event",
                "description": "Create a calendar event.",
                "defer_loading": true,
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"}
                    },
                    "required": ["title"],
                    "additionalProperties": false,
                }
            })],
        }
    );

    assert_eq!(
        serde_json::to_value(input)?,
        serde_json::json!({
            "type": "tool_search_output",
            "call_id": "search-1",
            "status": "completed",
            "execution": "client",
            "tools": [{
                "type": "function",
                "name": "mcp__praxis_apps__calendar_create_event",
                "description": "Create a calendar event.",
                "defer_loading": true,
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"}
                    },
                    "required": ["title"],
                    "additionalProperties": false,
                }
            }]
        })
    );

    Ok(())
}

#[test]
fn tool_search_server_items_allow_null_call_id() -> Result<()> {
    let parsed_call: ResponseItem = serde_json::from_str(
        r#"{
            "type": "tool_search_call",
            "execution": "server",
            "call_id": null,
            "status": "completed",
            "arguments": {
                "paths": ["crm"]
            }
        }"#,
    )?;
    assert_eq!(
        parsed_call,
        ResponseItem::ToolSearchCall {
            id: None,
            call_id: None,
            status: Some("completed".to_string()),
            execution: "server".to_string(),
            arguments: serde_json::json!({
                "paths": ["crm"],
            }),
        }
    );

    let parsed_output: ResponseItem = serde_json::from_str(
        r#"{
            "type": "tool_search_output",
            "execution": "server",
            "call_id": null,
            "status": "completed",
            "tools": []
        }"#,
    )?;
    assert_eq!(
        parsed_output,
        ResponseItem::ToolSearchOutput {
            call_id: None,
            status: "completed".to_string(),
            execution: "server".to_string(),
            tools: vec![],
        }
    );

    Ok(())
}

#[path = "models_tests/function_call_output.rs"]
mod function_call_output;
#[path = "models_tests/local_images.rs"]
mod local_images;
