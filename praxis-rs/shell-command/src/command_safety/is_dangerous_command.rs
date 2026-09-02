use crate::bash::parse_shell_lc_plain_commands;
use crate::command_safety::identity::executable_name;

#[cfg(windows)]
#[path = "windows_dangerous_commands.rs"]
mod windows_dangerous_commands;

/// Reports commands that require an explicit safety decision before execution.
pub fn command_might_be_dangerous(command: &[String]) -> bool {
    #[cfg(windows)]
    if windows_dangerous_commands::is_dangerous_command_windows(command) {
        return true;
    }

    is_dangerous_exec(command)
        || parse_shell_lc_plain_commands(command)
            .is_some_and(|commands| commands.iter().any(|nested| is_dangerous_exec(nested)))
}

fn is_dangerous_exec(command: &[String]) -> bool {
    let Some(executable) = command.first().and_then(|raw| executable_name(raw)) else {
        return false;
    };
    match executable.as_ref() {
        "rm" => matches!(command.get(1).map(String::as_str), Some("-f" | "-rf")),
        "sudo" => is_dangerous_exec(&command[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn rm_rf_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&["rm", "-rf", "/"])));
    }

    #[test]
    fn rm_f_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&["rm", "-f", "/"])));
    }

    #[test]
    fn absolute_rm_path_is_dangerous() {
        assert!(command_might_be_dangerous(&vec_str(&[
            "/usr/bin/rm",
            "-rf",
            "/",
        ])));
    }
}
