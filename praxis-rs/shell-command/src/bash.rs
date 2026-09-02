mod syntax;

use std::path::Path;

use tree_sitter::Parser;
use tree_sitter::Tree;
use tree_sitter_bash::LANGUAGE as BASH;

use crate::shell_detect::ShellType;
use crate::shell_detect::detect_shell_type;
use syntax::Syntax;

/// Parses Bash-compatible source into a syntax tree.
pub fn try_parse_shell(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&BASH.into()).ok()?;
    parser.parse(source, None)
}

/// Projects a syntax tree only when every command is literal and joined by safe operators.
pub fn try_parse_word_only_commands_sequence(
    tree: &Tree,
    source: &str,
) -> Option<Vec<Vec<String>>> {
    Syntax::new(source).plain_sequence(tree)
}

/// Extracts a Bash-compatible executable and its inline script.
pub fn extract_bash_command(command: &[String]) -> Option<(&str, &str)> {
    let [executable, flag, source] = command else {
        return None;
    };
    if !matches!(flag.as_str(), "-c" | "-lc") {
        return None;
    }
    matches!(
        detect_shell_type(Path::new(executable)),
        Some(ShellType::Bash | ShellType::Sh | ShellType::Zsh)
    )
    .then_some((executable.as_str(), source.as_str()))
}

/// Parses a Bash-compatible invocation into literal command arguments.
pub fn parse_shell_lc_plain_commands(command: &[String]) -> Option<Vec<Vec<String>>> {
    let (_, source) = extract_bash_command(command)?;
    let tree = try_parse_shell(source)?;
    Syntax::new(source).plain_sequence(&tree)
}

/// Extracts the argv prefix from a single literal command carrying a heredoc.
pub fn parse_shell_lc_single_command_prefix(command: &[String]) -> Option<Vec<String>> {
    let (_, source) = extract_bash_command(command)?;
    let tree = try_parse_shell(source)?;
    Syntax::new(source).single_heredoc_prefix(&tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn parse_seq(src: &str) -> Option<Vec<Vec<String>>> {
        let tree = try_parse_shell(src)?;
        try_parse_word_only_commands_sequence(&tree, src)
    }

    #[test]
    fn accepts_single_simple_command() {
        let cmds = parse_seq("ls -1").unwrap();
        assert_eq!(cmds, vec![vec!["ls".to_string(), "-1".to_string()]]);
    }

    #[test]
    fn accepts_multiple_commands_with_allowed_operators() {
        let src = "ls && pwd; echo 'hi there' | wc -l";
        let cmds = parse_seq(src).unwrap();
        let expected: Vec<Vec<String>> = vec![
            vec!["ls".to_string()],
            vec!["pwd".to_string()],
            vec!["echo".to_string(), "hi there".to_string()],
            vec!["wc".to_string(), "-l".to_string()],
        ];
        assert_eq!(cmds, expected);
    }

    #[test]
    fn extracts_double_and_single_quoted_strings() {
        let cmds = parse_seq("echo \"hello world\"").unwrap();
        assert_eq!(
            cmds,
            vec![vec!["echo".to_string(), "hello world".to_string()]]
        );

        let cmds2 = parse_seq("echo 'hi there'").unwrap();
        assert_eq!(
            cmds2,
            vec![vec!["echo".to_string(), "hi there".to_string()]]
        );
    }

    #[test]
    fn accepts_double_quoted_strings_with_newlines() {
        let cmds = parse_seq("git commit -m \"line1\nline2\"").unwrap();
        assert_eq!(
            cmds,
            vec![vec![
                "git".to_string(),
                "commit".to_string(),
                "-m".to_string(),
                "line1\nline2".to_string(),
            ]]
        );
    }

    #[test]
    fn accepts_mixed_quote_concatenation() {
        assert_eq!(
            parse_seq(r#"echo "/usr"'/'"local"/bin"#).unwrap(),
            vec![vec!["echo".to_string(), "/usr/local/bin".to_string()]]
        );
        assert_eq!(
            parse_seq(r#"echo '/usr'"/"'local'/bin"#).unwrap(),
            vec![vec!["echo".to_string(), "/usr/local/bin".to_string()]]
        );
    }

    #[test]
    fn rejects_double_quoted_strings_with_expansions() {
        assert!(parse_seq(r#"echo "hi ${USER}""#).is_none());
        assert!(parse_seq(r#"echo "$HOME""#).is_none());
    }

    #[test]
    fn accepts_numbers_as_words() {
        let cmds = parse_seq("echo 123 456").unwrap();
        assert_eq!(
            cmds,
            vec![vec![
                "echo".to_string(),
                "123".to_string(),
                "456".to_string()
            ]]
        );
    }

    #[test]
    fn rejects_parentheses_and_subshells() {
        assert!(parse_seq("(ls)").is_none());
        assert!(parse_seq("ls || (pwd && echo hi)").is_none());
    }

    #[test]
    fn rejects_redirections_and_unsupported_operators() {
        assert!(parse_seq("ls > out.txt").is_none());
        assert!(parse_seq("echo hi & echo bye").is_none());
    }

    #[test]
    fn rejects_command_and_process_substitutions_and_expansions() {
        assert!(parse_seq("echo $(pwd)").is_none());
        assert!(parse_seq("echo `pwd`").is_none());
        assert!(parse_seq("echo $HOME").is_none());
        assert!(parse_seq("echo \"hi $USER\"").is_none());
    }

    #[test]
    fn rejects_variable_assignment_prefix() {
        assert!(parse_seq("FOO=bar ls").is_none());
    }

    #[test]
    fn rejects_comments_from_plain_command_sequences() {
        assert!(parse_seq("ls # output is intentionally ignored").is_none());
    }

    #[test]
    fn rejects_trailing_operator_parse_error() {
        assert!(parse_seq("ls &&").is_none());
    }

    #[test]
    fn rejects_empty_command_position_with_leading_operator() {
        assert!(parse_seq("&& ls").is_none());
    }

    #[test]
    fn rejects_empty_command_position_with_double_separator() {
        assert!(parse_seq("ls ;; pwd").is_none());
    }

    #[test]
    fn rejects_empty_command_position_with_empty_pipeline_segment() {
        assert!(parse_seq("ls | | wc").is_none());
    }

    #[test]
    fn parse_zsh_lc_plain_commands() {
        let command = vec!["zsh".to_string(), "-lc".to_string(), "ls".to_string()];
        let parsed = parse_shell_lc_plain_commands(&command).unwrap();
        assert_eq!(parsed, vec![vec!["ls".to_string()]]);
    }

    #[test]
    fn accepts_concatenated_flag_and_value() {
        // Test case: -g"*.py" (flag directly concatenated with quoted value)
        let cmds = parse_seq("rg -n \"foo\" -g\"*.py\"").unwrap();
        assert_eq!(
            cmds,
            vec![vec![
                "rg".to_string(),
                "-n".to_string(),
                "foo".to_string(),
                "-g*.py".to_string(),
            ]]
        );
    }

    #[test]
    fn accepts_concatenated_flag_with_single_quotes() {
        let cmds = parse_seq("grep -n 'pattern' -g'*.txt'").unwrap();
        assert_eq!(
            cmds,
            vec![vec![
                "grep".to_string(),
                "-n".to_string(),
                "pattern".to_string(),
                "-g*.txt".to_string(),
            ]]
        );
    }

    #[test]
    fn rejects_concatenation_with_variable_substitution() {
        // Environment variables in concatenated strings should be rejected
        assert!(parse_seq("rg -g\"$VAR\" pattern").is_none());
        assert!(parse_seq("rg -g\"${VAR}\" pattern").is_none());
    }

    #[test]
    fn rejects_concatenation_with_command_substitution() {
        // Command substitution in concatenated strings should be rejected
        assert!(parse_seq("rg -g\"$(pwd)\" pattern").is_none());
        assert!(parse_seq("rg -g\"$(echo '*.py')\" pattern").is_none());
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_supports_heredoc() {
        let command = vec![
            "zsh".to_string(),
            "-lc".to_string(),
            "python3 <<'PY'\nprint('hello')\nPY".to_string(),
        ];
        let parsed = parse_shell_lc_single_command_prefix(&command);
        assert_eq!(parsed, Some(vec!["python3".to_string()]));

        let command_unquoted = vec![
            "zsh".to_string(),
            "-lc".to_string(),
            "python3 << PY\nprint('hello')\nPY".to_string(),
        ];
        let parsed_unquoted = parse_shell_lc_single_command_prefix(&command_unquoted);
        assert_eq!(parsed_unquoted, Some(vec!["python3".to_string()]));
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_rejects_multi_command_scripts() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "python3 <<'PY'\nprint('hello')\nPY\necho done".to_string(),
        ];
        assert_eq!(parse_shell_lc_single_command_prefix(&command), None);
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_rejects_non_heredoc_redirects() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "echo hello > /tmp/out.txt".to_string(),
        ];
        assert_eq!(parse_shell_lc_single_command_prefix(&command), None);
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_accepts_heredoc_with_extra_redirect() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "python3 <<'PY' > /tmp/out.txt\nprint('hello')\nPY".to_string(),
        ];
        assert_eq!(
            parse_shell_lc_single_command_prefix(&command),
            Some(vec!["python3".to_string()])
        );
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_rejects_herestring_with_chaining() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            r#"echo hello > /tmp/out.txt && cat /tmp/out.txt"#.to_string(),
        ];
        assert_eq!(parse_shell_lc_single_command_prefix(&command), None);
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_rejects_herestring_with_substitution() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            r#"python3 <<< "$(rm -rf /)""#.to_string(),
        ];
        assert_eq!(parse_shell_lc_single_command_prefix(&command), None);
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_rejects_arithmetic_shift_non_heredoc_script() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "echo $((1<<2))".to_string(),
        ];
        assert_eq!(parse_shell_lc_single_command_prefix(&command), None);
    }

    #[test]
    fn parse_shell_lc_single_command_prefix_rejects_heredoc_command_with_word_expansion() {
        let command = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "python3 $((1<<2)) <<'PY'\nprint('hello')\nPY".to_string(),
        ];
        assert_eq!(parse_shell_lc_single_command_prefix(&command), None);
    }
}
