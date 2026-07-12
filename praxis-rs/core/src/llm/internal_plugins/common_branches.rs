use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::provider_model_catalog;
use crate::llm::profiles::claude;
use crate::llm::profiles::openrouter;

pub(super) fn register_common_branches(registry: &mut LlmPluginRegistryBuilder) {
    let openrouter_profile = openrouter::profile();
    let claude_profile = claude::profile();
    registry.add_model_catalog(provider_model_catalog(
        "anthropic-claude-models",
        "Anthropic Claude models",
        claude::is_first_party_provider,
        claude::is_first_party_model,
    ));
    #[cfg(test)]
    {
        register_common_branch(
            registry,
            "openrouter",
            "OpenRouter common branch prompt layer",
            openrouter_profile,
        );
        register_common_branch(
            registry,
            "claude",
            "Claude common branch prompt layer",
            claude_profile,
        );
    }

    #[cfg(not(test))]
    {
        registry.add_profile(openrouter_profile);
        registry.add_profile(claude_profile);
    }
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
fn common_branch_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter",
        "claude" => "common/claude",
        _ => namespace,
    }
}

#[cfg(test)]
fn common_branch_prompt_layer_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/prompts",
        "claude" => "common/claude/prompts",
        _ => namespace,
    }
}

#[cfg(test)]
fn common_branch_task_policy_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/tasks",
        "claude" => "common/claude/tasks",
        _ => namespace,
    }
}

#[cfg(test)]
fn common_branch_tool_policy_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/tools",
        "claude" => "common/claude/tools",
        _ => namespace,
    }
}
