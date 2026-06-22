use praxis_protocol::user_input::UserInput;

pub(super) fn model_request_messages(input: &[UserInput]) -> Vec<String> {
    input
        .iter()
        .map(|item| match item {
            UserInput::Text { text, .. } => text.clone(),
            UserInput::Image { image_url } => format!("[image: {image_url}]"),
            UserInput::LocalImage { path } => format!("[local image: {}]", path.display()),
            UserInput::Skill { name, path } => format!("[skill: {name} at {}]", path.display()),
            UserInput::Mention { name, path } => format!("[mention: {name} at {path}]"),
            _ => format!("{item:?}"),
        })
        .collect()
}
