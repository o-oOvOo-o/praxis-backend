pub(super) fn format_agent_base_name(name: &str, base_name_reset_count: usize) -> String {
    match base_name_reset_count {
        0 => name.to_string(),
        reset_count if !name.is_ascii() => {
            let value = reset_count + 1;
            format!("{name}{value}")
        }
        reset_count => {
            let value = reset_count + 1;
            let suffix = match value % 100 {
                11..=13 => "th",
                _ => match value % 10 {
                    1 => "st", // codespell:ignore
                    2 => "nd", // codespell:ignore
                    3 => "rd", // codespell:ignore
                    _ => "th", // codespell:ignore
                },
            };
            format!("{name} the {value}{suffix}")
        }
    }
}
