use praxis_protocol::models::ResponseItem;

pub(super) fn build_initial_context_items(
    developer_sections: Vec<String>,
    contextual_user_sections: Vec<String>,
    separate_guardian_developer_message: bool,
    developer_instructions: Option<&str>,
) -> Vec<ResponseItem> {
    let mut items = Vec::with_capacity(3);
    if let Some(developer_message) =
        crate::context_manager::updates::build_developer_update_item(developer_sections)
    {
        items.push(developer_message);
    }
    if let Some(contextual_user_message) =
        crate::context_manager::updates::build_contextual_user_message(contextual_user_sections)
    {
        items.push(contextual_user_message);
    }
    if separate_guardian_developer_message
        && let Some(developer_instructions) = developer_instructions
        && let Some(guardian_developer_message) =
            crate::context_manager::updates::build_developer_update_item(vec![
                developer_instructions.to_string(),
            ])
    {
        items.push(guardian_developer_message);
    }
    items
}
