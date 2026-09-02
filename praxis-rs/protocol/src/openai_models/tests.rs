use super::*;
use pretty_assertions::assert_eq;

fn test_model(spec: Option<ModelMessages>) -> ModelInfo {
    ModelInfo {
        slug: "test-model".to_string(),
        display_name: "Test Model".to_string(),
        description: None,
        default_reasoning_level: None,
        supported_reasoning_levels: vec![],
        shell_type: ConfigShellToolType::ShellCommand,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority: 1,
        availability_nux: None,
        upgrade: None,
        base_instructions: "base".to_string(),
        model_messages: spec,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: None,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: None,
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: vec![],
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: false,
        supports_search_tool: false,
        multi_agent_version: None,
    }
}

fn personality_variables() -> ModelInstructionsVariables {
    ModelInstructionsVariables {
        personality_default: Some("default".to_string()),
        personality_friendly: Some("friendly".to_string()),
        personality_pragmatic: Some("pragmatic".to_string()),
    }
}

#[test]
fn reasoning_effort_from_str_accepts_known_values() {
    assert_eq!("high".parse(), Ok(ReasoningEffort::High));
    assert_eq!("minimal".parse(), Ok(ReasoningEffort::Minimal));
    assert_eq!("max".parse(), Ok(ReasoningEffort::Max));
    assert_eq!("ultra".parse(), Ok(ReasoningEffort::Ultra));
}

#[test]
fn reasoning_effort_from_str_preserves_unknown_values() {
    assert_eq!(
        "future".parse::<ReasoningEffort>(),
        Ok(ReasoningEffort::Custom("future".to_string()))
    );
}

#[test]
fn unknown_multi_agent_version_is_treated_as_unsupported() {
    let mut value = serde_json::to_value(test_model(None)).unwrap();
    value["multi_agent_version"] = serde_json::json!("v3");

    let model: ModelInfo = serde_json::from_value(value).unwrap();

    assert_eq!(model.multi_agent_version, None);
}

#[test]
fn known_gpt56_models_match_official_reasoning_capabilities() {
    let sol = known_openai_compatible_model_info("gpt-5.6-sol").unwrap();
    let terra = known_openai_compatible_model_info("gpt-5.6-terra").unwrap();
    let luna = known_openai_compatible_model_info("gpt-5.6-luna").unwrap();

    assert_eq!(sol.default_reasoning_level, Some(ReasoningEffort::Medium));
    assert_eq!(terra.default_reasoning_level, Some(ReasoningEffort::Medium));
    assert_eq!(luna.default_reasoning_level, Some(ReasoningEffort::Medium));
    assert_eq!(sol.default_reasoning_summary, ReasoningSummary::Auto);
    assert_eq!(terra.default_reasoning_summary, ReasoningSummary::Auto);
    assert_eq!(luna.default_reasoning_summary, ReasoningSummary::Auto);
    assert_eq!(sol.context_window, Some(372_000));
    assert_eq!(sol.multi_agent_version, Some(MultiAgentVersion::V2));
    assert_eq!(terra.multi_agent_version, Some(MultiAgentVersion::V2));
    assert_eq!(luna.multi_agent_version, Some(MultiAgentVersion::V1));
    assert_eq!(sol.base_instructions, BASE_INSTRUCTIONS_GPT_5_6);
    assert_eq!(terra.base_instructions, BASE_INSTRUCTIONS_GPT_5_6);
    assert_eq!(luna.base_instructions, BASE_INSTRUCTIONS_GPT_5_6);
    assert!(sol.model_messages.is_some());
    assert!(sol.base_instructions.contains("## Context efficiency"));
    assert!(sol.base_instructions.contains("xN count"));
    assert!(
        sol.supported_reasoning_levels
            .iter()
            .any(|p| p.effort == ReasoningEffort::Ultra)
    );
    assert!(
        terra
            .supported_reasoning_levels
            .iter()
            .any(|p| p.effort == ReasoningEffort::Ultra)
    );
    assert!(
        luna.supported_reasoning_levels
            .iter()
            .any(|p| p.effort == ReasoningEffort::Max)
    );
    assert!(
        !luna
            .supported_reasoning_levels
            .iter()
            .any(|p| p.effort == ReasoningEffort::Ultra)
    );
}

#[test]
fn deepseek_v4_flash_is_available_in_the_local_picker_catalog() {
    let models = known_openai_compatible_picker_model_infos();
    let flash = models
        .iter()
        .find(|model| model.slug == "deepseek-v4-flash")
        .expect("DeepSeek V4 Flash should be available before remote catalog refresh");

    assert_eq!(flash.display_name, "DeepSeek V4 Flash");
    assert_eq!(flash.default_reasoning_level, Some(ReasoningEffort::High));
    assert_eq!(flash.context_window, Some(1_000_000));
}

#[test]
fn gpt56_prompt_does_not_leak_into_older_models() {
    let model = known_openai_compatible_model_info("gpt-5.5").unwrap();

    assert_eq!(model.base_instructions, BASE_INSTRUCTIONS_DEFAULT);
    assert!(model.model_messages.is_none());
}

#[test]
fn get_model_instructions_uses_template_when_placeholder_present() {
    let model = test_model(Some(ModelMessages {
        instructions_template: Some("Hello {{ personality }}".to_string()),
        instructions_variables: Some(personality_variables()),
    }));

    let instructions = model.get_model_instructions(Some(Personality::Friendly));

    assert_eq!(instructions, "Hello friendly");
}

#[test]
fn get_model_instructions_always_strips_placeholder() {
    let model = test_model(Some(ModelMessages {
        instructions_template: Some("Hello\n{{ personality }}".to_string()),
        instructions_variables: Some(ModelInstructionsVariables {
            personality_default: None,
            personality_friendly: Some("friendly".to_string()),
            personality_pragmatic: None,
        }),
    }));
    assert_eq!(
        model.get_model_instructions(Some(Personality::Friendly)),
        "Hello\nfriendly"
    );
    assert_eq!(
        model.get_model_instructions(Some(Personality::Pragmatic)),
        "Hello\n"
    );
    assert_eq!(
        model.get_model_instructions(Some(Personality::None)),
        "Hello\n"
    );
    assert_eq!(
        model.get_model_instructions(/*personality*/ None),
        "Hello\n"
    );

    let model_no_personality = test_model(Some(ModelMessages {
        instructions_template: Some("Hello\n{{ personality }}".to_string()),
        instructions_variables: Some(ModelInstructionsVariables {
            personality_default: None,
            personality_friendly: None,
            personality_pragmatic: None,
        }),
    }));
    assert_eq!(
        model_no_personality.get_model_instructions(Some(Personality::Friendly)),
        "Hello\n"
    );
    assert_eq!(
        model_no_personality.get_model_instructions(Some(Personality::Pragmatic)),
        "Hello\n"
    );
    assert_eq!(
        model_no_personality.get_model_instructions(Some(Personality::None)),
        "Hello\n"
    );
    assert_eq!(
        model_no_personality.get_model_instructions(/*personality*/ None),
        "Hello\n"
    );
}

#[test]
fn get_model_instructions_falls_back_when_template_is_missing() {
    let model = test_model(Some(ModelMessages {
        instructions_template: None,
        instructions_variables: Some(ModelInstructionsVariables {
            personality_default: None,
            personality_friendly: None,
            personality_pragmatic: None,
        }),
    }));

    let instructions = model.get_model_instructions(Some(Personality::Friendly));

    assert_eq!(instructions, "base");
}

#[test]
fn get_personality_message_returns_default_when_personality_is_none() {
    let personality_template = personality_variables();
    assert_eq!(
        personality_template.get_personality_message(/*personality*/ None),
        Some("default".to_string())
    );
}

#[test]
fn get_personality_message() {
    let personality_variables = personality_variables();
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::Friendly)),
        Some("friendly".to_string())
    );
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::Pragmatic)),
        Some("pragmatic".to_string())
    );
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::None)),
        Some(String::new())
    );
    assert_eq!(
        personality_variables.get_personality_message(/*personality*/ None),
        Some("default".to_string())
    );

    let personality_variables = ModelInstructionsVariables {
        personality_default: Some("default".to_string()),
        personality_friendly: None,
        personality_pragmatic: None,
    };
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::Friendly)),
        None
    );
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::Pragmatic)),
        None
    );
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::None)),
        Some(String::new())
    );
    assert_eq!(
        personality_variables.get_personality_message(/*personality*/ None),
        Some("default".to_string())
    );

    let personality_variables = ModelInstructionsVariables {
        personality_default: None,
        personality_friendly: Some("friendly".to_string()),
        personality_pragmatic: Some("pragmatic".to_string()),
    };
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::Friendly)),
        Some("friendly".to_string())
    );
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::Pragmatic)),
        Some("pragmatic".to_string())
    );
    assert_eq!(
        personality_variables.get_personality_message(Some(Personality::None)),
        Some(String::new())
    );
    assert_eq!(
        personality_variables.get_personality_message(/*personality*/ None),
        None
    );
}

#[test]
fn model_info_defaults_availability_nux_to_none_when_omitted() {
    let model: ModelInfo = serde_json::from_value(serde_json::json!({
        "slug": "test-model",
        "display_name": "Test Model",
        "description": null,
        "supported_reasoning_levels": [],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "upgrade": null,
        "base_instructions": "base",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "default_reasoning_summary": "auto",
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {
            "mode": "bytes",
            "limit": 10000
        },
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": null,
        "auto_compact_token_limit": null,
        "effective_context_window_percent": 95,
        "experimental_supported_tools": [],
        "input_modalities": ["text", "image"]
    }))
    .expect("deserialize model info");

    assert_eq!(model.availability_nux, None);
    assert!(!model.supports_image_detail_original);
    assert_eq!(model.web_search_tool_type, WebSearchToolType::Text);
    assert!(!model.supports_search_tool);
}

#[test]
fn model_preset_preserves_availability_nux() {
    let preset = ModelPreset::from(ModelInfo {
        availability_nux: Some(ModelAvailabilityNux {
            message: "Try Spark.".to_string(),
        }),
        ..test_model(/*spec*/ None)
    });

    assert_eq!(
        preset.availability_nux,
        Some(ModelAvailabilityNux {
            message: "Try Spark.".to_string(),
        })
    );
}
