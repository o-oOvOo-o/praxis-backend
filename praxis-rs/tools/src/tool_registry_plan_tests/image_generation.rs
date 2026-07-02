use super::*;

#[test]
fn image_generation_tools_use_praxis_routed_backend() {
    let mut supported_model_info = model_info();
    supported_model_info.experimental_supported_tools = vec!["image_generation".to_string()];
    let mut unsupported_model_info = supported_model_info.clone();
    unsupported_model_info.experimental_supported_tools.clear();
    let default_features = Features::with_defaults();

    let available_models = Vec::new();
    let default_tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &supported_model_info,
        available_models: &available_models,
        features: &default_features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (default_tools, _) = build_specs(
        &default_tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );
    assert!(
        default_tools
            .iter()
            .any(|tool| tool.spec.name() == "image_generation"),
        "image_generation should be enabled by default for supported models"
    );

    let supported_tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &supported_model_info,
        available_models: &available_models,
        features: &default_features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (supported_tools, _) = build_specs(
        &supported_tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );
    assert_contains_tool_names(&supported_tools, &["image_generation"]);
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &unsupported_model_info,
        available_models: &available_models,
        features: &default_features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let (tools, handlers) = build_specs(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );
    assert_contains_tool_names(&tools, &["image_generation"]);
    assert_routed_image_generation_tool(find_tool(&supported_tools, "image_generation"));
    assert!(
        handlers
            .iter()
            .any(|handler| handler.name == "image_generation"
                && handler.kind == ToolHandlerKind::ImageGeneration),
        "routed image_generation should register a core handler"
    );
    let routed_image_tool = find_tool(&tools, "image_generation");
    assert_routed_image_generation_tool(routed_image_tool);
    assert!(
        handlers
            .iter()
            .any(|handler| handler.name == "image_generation"
                && handler.kind == ToolHandlerKind::ImageGeneration),
        "routed image_generation should register a core handler"
    );
}

fn assert_routed_image_generation_tool(routed_image_tool: &ConfiguredTool) {
    assert_eq!(
        serde_json::to_value(&routed_image_tool.spec).expect("serialize routed image tool"),
        serde_json::json!({
            "type": "function",
            "name": "image_generation",
            "description": "Generate an image from a prompt. Praxis routes this through an OpenAI image-capable Responses model and returns the saved local image path.",
            "strict": false,
            "parameters": {
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Detailed natural-language prompt for the image to generate."
                    },
                    "size": {
                        "type": "string",
                        "description": "Optional requested size or aspect ratio, such as 1024x1024, 1536x1024, portrait, landscape, or square."
                    },
                    "quality": {
                        "type": "string",
                        "description": "Optional requested quality or rendering intent, such as draft, standard, high, or production."
                    }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }
        })
    );
}
