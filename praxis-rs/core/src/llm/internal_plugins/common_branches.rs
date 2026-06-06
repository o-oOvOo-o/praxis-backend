use crate::llm::ids::BehaviorProfileId;
use crate::llm::ids::WireId;
use crate::llm::internal_plugins::LlmExtensionDescriptor;
use crate::llm::internal_plugins::LlmExtensionKind;
use crate::llm::internal_plugins::LlmPluginRegistryBuilder;
use crate::llm::internal_plugins::behavior_extension;
use crate::llm::internal_plugins::wire_extension;
use crate::llm::profiles::openrouter;
use crate::llm::wire::plugin::WireDescriptor;

pub(crate) fn register_common_branches(registry: &mut LlmPluginRegistryBuilder) {
    registry.add_wire(WireDescriptor {
        id: WireId::OpenAiCompat,
        name: "OpenAI-compatible Chat Completions",
    });
    registry.add_extension(wire_extension(
        WireId::OpenAiCompat,
        "OpenAI-compatible Chat Completions",
    ));

    register_common_branch(
        registry,
        BehaviorProfileId::OpenRouter,
        "openrouter",
        "OpenRouter",
        "OpenRouter common branch prompt layer",
        openrouter::profile(),
    );
    register_claude_placeholder(registry);
}

fn register_common_branch(
    registry: &mut LlmPluginRegistryBuilder,
    behavior: BehaviorProfileId,
    namespace: &'static str,
    label: &'static str,
    prompt_label: &'static str,
    profile: crate::llm::profiles::plugin::ProfileDescriptor,
) {
    registry.add_extension(LlmExtensionDescriptor::new(
        LlmExtensionKind::Provider,
        common_branch_id(namespace),
        label,
    ));
    registry.add_extension(behavior_extension(behavior, label));
    registry.add_extension(LlmExtensionDescriptor::new(
        LlmExtensionKind::PromptLayer,
        common_branch_prompt_layer_id(namespace),
        prompt_label,
    ));
    registry.add_extension(LlmExtensionDescriptor::new(
        LlmExtensionKind::TaskPolicy,
        common_branch_task_policy_id(namespace),
        "Common branch task policy",
    ));
    registry.add_extension(LlmExtensionDescriptor::new(
        LlmExtensionKind::ToolPolicy,
        common_branch_tool_policy_id(namespace),
        "Common branch tool dialect",
    ));
    registry.add_profile(profile);
}

fn register_claude_placeholder(registry: &mut LlmPluginRegistryBuilder) {
    registry.add_extension(LlmExtensionDescriptor::new(
        LlmExtensionKind::Provider,
        "common/claude_placeholder",
        "Claude placeholder: Anthropic statement",
    ));
    registry.add_extension(LlmExtensionDescriptor::new(
        LlmExtensionKind::PromptLayer,
        "common/claude_placeholder/statement",
        "Praxis does not provide a Claude adapter",
    ));
}

fn common_branch_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter",
        _ => namespace,
    }
}

fn common_branch_prompt_layer_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/prompts",
        _ => namespace,
    }
}

fn common_branch_task_policy_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/tasks",
        _ => namespace,
    }
}

fn common_branch_tool_policy_id(namespace: &'static str) -> &'static str {
    match namespace {
        "openrouter" => "common/openrouter/tools",
        _ => namespace,
    }
}
