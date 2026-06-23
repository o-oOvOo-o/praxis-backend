pub(in crate::agent_os::classification) fn extract_port(command: &str) -> Option<u16> {
    for marker in ["--port ", "-p "] {
        if let Some((_, suffix)) = command.split_once(marker) {
            let digits: String = suffix
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if let Ok(port) = digits.parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}
