pub(super) fn is_test_command(command: &str) -> bool {
    command.contains(" test")
        || command.contains("cargo nextest")
        || command.contains("pytest")
        || command.contains("vitest")
        || command.contains("jest")
        || command.contains("go test")
}

pub(super) fn is_harness_command(command: &str) -> bool {
    [
        "harness",
        "native_harness",
        "parity_harness",
        "compare_harness",
        "target/debug/",
        "target\\debug\\",
        "target/release/",
        "target\\release\\",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_gpu_command(command: &str) -> bool {
    [
        "gpu", "cuda", "nvidia", "vulkan", "wgpu", "directx", "d3d12", "metal",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_compile_command(command: &str) -> bool {
    [
        "cargo build",
        "cargo check",
        "cargo run",
        "npm run build",
        "pnpm build",
        "pnpm turbo build",
        "yarn build",
        "just build",
        "ninja",
        "bazel build",
        "make",
        "cmake --build",
        "maturin",
        "python setup.py build",
        "dotnet build",
        "msbuild",
        "gradle build",
        "mvn package",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_run_app_command(command: &str) -> bool {
    [
        "npm run dev",
        "pnpm dev",
        "yarn dev",
        "vite",
        "next dev",
        "cargo run",
        "trunk serve",
        "python -m http.server",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_network_command(command: &str) -> bool {
    [
        "curl ",
        "wget ",
        "git clone",
        "npm install",
        "pnpm install",
        "yarn install",
        "cargo fetch",
        "pip install",
        "uv pip install",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_file_write_command(command: &str) -> bool {
    [
        "apply_patch",
        "set-content",
        "out-file",
        "new-item",
        "remove-item",
        "move-item",
        "copy-item",
        "python -c",
        "node -e",
        "tee ",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn has_file_redirection(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'>' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        if cursor < bytes.len() && bytes[cursor] == b'>' {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'&' {
            index = cursor + 1;
            continue;
        }
        return true;
    }
    false
}

pub(super) fn is_git_mutation(command: &str) -> bool {
    [
        "git commit",
        "git rebase",
        "git merge",
        "git checkout",
        "git switch",
        "git reset",
        "git clean",
        "git stash",
        "git add",
        "git rm",
        "git mv",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_long_process_command(command: &str) -> bool {
    [
        "watch ",
        "tail -f",
        "sleep ",
        "python train.py",
        "tensorboard",
        "jupyter",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn is_read_only_command(command: &str) -> bool {
    [
        "rg ",
        "grep ",
        "get-content",
        "select-string",
        "ls",
        "dir",
        "git status",
        "git diff",
        "git show",
        "git log",
        "findstr",
        "type ",
        "cat ",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(super) fn extract_port(command: &str) -> Option<u16> {
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
