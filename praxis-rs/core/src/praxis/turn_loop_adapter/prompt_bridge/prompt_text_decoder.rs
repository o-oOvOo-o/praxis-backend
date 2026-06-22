pub(super) fn prompt_text_item_from_role(
    role: &str,
    text: String,
) -> praxis_loop::model::PromptItem {
    match role {
        "user" => praxis_loop::model::PromptItem::UserText(text),
        "assistant" => praxis_loop::model::PromptItem::AssistantText(text),
        _ => praxis_loop::model::PromptItem::SystemText(text),
    }
}
