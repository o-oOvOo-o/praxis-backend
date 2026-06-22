use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::profiles::openrouter;

pub(super) fn register_common_branches(registry: &mut LlmPluginRegistryBuilder) {
    let profile = openrouter::profile();
    #[cfg(test)]
    {
        register_common_branch(
            registry,
            "openrouter",
            "OpenRouter common branch prompt layer",
            profile,
        );
        register_claude_placeholder(registry);
    }

    #[cfg(not(test))]
    registry.add_profile(profile);
}

#[cfg(test)]
fn register_common_branch(
    registry: &mut LlmPluginRegistryBuilder,
    namespace: &'static str,
    prompt_label: &'static str,
    profile: crate::llm::profiles::plugin::ProfileDescriptor,
) {
    registry.add_provider_extension(common_branch_id(namespace), profile.label);
    registry.add_profile_extension_bundle(
        profile,
        (common_branch_prompt_layer_id(namespace), prompt_label),
        (
            common_branch_task_policy_id(namespace),
            "Common branch task policy",
        ),
        (
            common_branch_tool_policy_id(namespace),
            "Common branch tool dialect",
        ),
    );
    registry.add_profile(profile);
}

#[cfg(test)]
fn register_claude_placeholder(registry: &mut LlmPluginRegistryBuilder) {
    registry.add_provider_extension(
        "common/claude_placeholder",
        "Claude placeholder: Anthropic statement",
    );
    registry.add_prompt_layer_extension(
        "common/claude_placeholder/statement",
        "Praxis does not provide a Claude adapter",
    );
}

#[cfg(test)]
fn common_branch_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter",
        _ => namespace,
    }
}

#[cfg(test)]
fn common_branch_prompt_layer_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/prompts",
        _ => namespace,
    }
}

#[cfg(test)]
fn common_branch_task_policy_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/tasks",
        _ => namespace,
    }
}

#[cfg(test)]
fn common_branch_tool_policy_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/tools",
        _ => namespace,
    }
}
