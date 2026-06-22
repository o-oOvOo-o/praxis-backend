use praxis_tools::ToolSpec as CoreToolSpec;

pub(in crate::praxis::turn_loop_adapter) fn core_tool_description(spec: &CoreToolSpec) -> String {
    match spec {
        CoreToolSpec::Function(tool) => tool.description.clone(),
        CoreToolSpec::ToolSearch { description, .. } => description.clone(),
        CoreToolSpec::LocalShell {} => "Run a local shell command".to_string(),
        CoreToolSpec::ImageGeneration { .. } => "Generate an image".to_string(),
        CoreToolSpec::WebSearch { .. } => "Search the web".to_string(),
        CoreToolSpec::Freeform(tool) => tool.description.clone(),
    }
}
