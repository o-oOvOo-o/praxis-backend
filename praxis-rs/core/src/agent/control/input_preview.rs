use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::Op;
use praxis_protocol::user_input::UserInput;

pub(crate) fn render_input_preview(initial_operation: &Op) -> String {
    match initial_operation {
        Op::UserInput { items, .. } => items
            .iter()
            .map(|item| match item {
                UserInput::Text { text, .. } => text.clone(),
                UserInput::Image { .. } => "[image]".to_string(),
                UserInput::LocalImage { path } => format!("[local_image:{}]", path.display()),
                UserInput::Skill { name, path } => format!("[skill:${name}]({})", path.display()),
                UserInput::Mention { name, path } => format!("[mention:${name}]({path})"),
                _ => "[input]".to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Op::InterAgentCommunication {
            communication: InterAgentCommunication { content, .. },
        } => content.clone(),
        _ => String::new(),
    }
}
