use super::*;

#[test]
fn command_families_share_one_summary_contract() {
    for command in ["ls -I target workspace/src", "eza -I target workspace/src"] {
        let parsed = parse_command(&shlex_split_safe(command));
        assert!(matches!(
            parsed.as_slice(),
            [ParsedCommand::ListFiles { path: Some(path), .. }] if path == "workspace"
        ));
    }

    for command in ["cat -- notes.txt", "more -- notes.txt"] {
        let parsed = parse_command(&shlex_split_safe(command));
        assert!(matches!(
            parsed.as_slice(),
            [ParsedCommand::Read { name, path, .. }]
                if name == "notes.txt" && path == &PathBuf::from("notes.txt")
        ));
    }
}

#[test]
fn only_adjacent_equivalent_projections_are_coalesced() {
    let repeated = shlex_split_safe("bash -lc 'rg needle src; rg needle src'");
    assert_eq!(parse_command(&repeated).len(), 1);

    let separated = shlex_split_safe("bash -lc 'rg needle src; ls files; rg needle src'");
    let parsed = parse_command(&separated);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed.first(), parsed.last());
}

#[test]
fn option_values_never_become_summary_operands() {
    assert_parsed(
        &shlex_split_safe("tree --charset utf-8 workspace/src"),
        vec![ParsedCommand::ListFiles {
            cmd: "tree --charset utf-8 workspace/src".to_string(),
            path: Some("workspace".to_string()),
        }],
    );
    assert_parsed(
        &shlex_split_safe("bat --language rust source/lib.rs"),
        vec![ParsedCommand::Read {
            cmd: "bat --language rust source/lib.rs".to_string(),
            name: "lib.rs".to_string(),
            path: PathBuf::from("source/lib.rs"),
        }],
    );
}
