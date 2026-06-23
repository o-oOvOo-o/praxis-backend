use super::*;
use std::path::PathBuf;

#[test]
fn bin_bash_lc_sed() {
    assert_parsed(
        &shlex_split_safe("/bin/bash -lc 'sed -n '1,10p' Cargo.toml'"),
        vec![ParsedCommand::Read {
            cmd: "sed -n '1,10p' Cargo.toml".to_string(),
            name: "Cargo.toml".to_string(),
            path: PathBuf::from("Cargo.toml"),
        }],
    );
}
#[test]
fn bin_zsh_lc_sed() {
    assert_parsed(
        &shlex_split_safe("/bin/zsh -lc 'sed -n '1,10p' Cargo.toml'"),
        vec![ParsedCommand::Read {
            cmd: "sed -n '1,10p' Cargo.toml".to_string(),
            name: "Cargo.toml".to_string(),
            path: PathBuf::from("Cargo.toml"),
        }],
    );
}

#[test]
fn powershell_command_is_stripped() {
    assert_parsed(
        &vec_str(&["powershell", "-Command", "Get-ChildItem"]),
        vec![ParsedCommand::Unknown {
            cmd: "Get-ChildItem".to_string(),
        }],
    );
}

#[test]
fn pwsh_with_noprofile_and_c_alias_is_stripped() {
    assert_parsed(
        &vec_str(&["pwsh", "-NoProfile", "-c", "Write-Host hi"]),
        vec![ParsedCommand::Unknown {
            cmd: "Write-Host hi".to_string(),
        }],
    );
}

#[test]
fn powershell_with_path_is_stripped() {
    let command = if cfg!(windows) {
        "C:\\windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
    } else {
        "/usr/local/bin/powershell.exe"
    };

    assert_parsed(
        &vec_str(&[command, "-NoProfile", "-c", "Write-Host hi"]),
        vec![ParsedCommand::Unknown {
            cmd: "Write-Host hi".to_string(),
        }],
    );
}
