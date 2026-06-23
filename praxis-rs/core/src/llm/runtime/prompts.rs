use std::fs;

use praxis_plugin::PluginLlmPromptSlot;

use super::normalization::normalize_selector;
use crate::llm::ids::BehaviorProfileId;
use crate::llm::prompts::LlmPromptPurpose;

pub(super) fn resolve_prompt_slot<'a>(
    slots: impl Iterator<Item = &'a PluginLlmPromptSlot>,
    purpose: LlmPromptPurpose,
) -> Option<&'a PluginLlmPromptSlot> {
    slots.into_iter().find(|slot| {
        let slot = normalize_selector(&slot.slot);
        purpose
            .slots()
            .iter()
            .any(|candidate| slot == normalize_selector(candidate))
    })
}

pub(super) fn join_optional_prompt_layers(
    base: Option<String>,
    product: Option<String>,
) -> Option<String> {
    match (base, product) {
        (Some(base), Some(product)) => Some(join_prompt_layers(&base, &product)),
        (Some(base), None) => Some(base),
        (None, Some(product)) => Some(product),
        (None, None) => None,
    }
}

fn join_prompt_layers(base: &str, product: &str) -> String {
    let base = base.trim();
    let product = product.trim();
    match (base.is_empty(), product.is_empty()) {
        (true, true) => String::new(),
        (true, false) => product.to_string(),
        (false, true) => base.to_string(),
        (false, false) => format!("{base}\n\n{product}"),
    }
}

pub(super) fn read_plugin_prompt(
    slot: &PluginLlmPromptSlot,
    owner_id: &str,
    behavior_id: BehaviorProfileId,
    purpose: LlmPromptPurpose,
) -> Option<String> {
    let contents = match fs::read_to_string(slot.path.as_path()) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %slot.path.display(),
                plugin_llm_owner = owner_id,
                prompt_profile = behavior_id.as_str(),
                prompt_purpose = purpose.as_str(),
                "failed to read plugin LLM prompt: {err}"
            );
            return None;
        }
    };
    let prompt = contents.trim();
    if prompt.is_empty() {
        tracing::warn!(
            path = %slot.path.display(),
            plugin_llm_owner = owner_id,
            prompt_profile = behavior_id.as_str(),
            prompt_purpose = purpose.as_str(),
            "ignoring empty plugin LLM prompt"
        );
        return None;
    }

    tracing::debug!(
        path = %slot.path.display(),
        plugin_llm_owner = owner_id,
        prompt_profile = behavior_id.as_str(),
        prompt_purpose = purpose.as_str(),
        "resolved plugin LLM prompt"
    );
    Some(prompt.to_string())
}
