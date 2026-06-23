pub(in crate::agent_os::classification) fn is_test_command(command: &str) -> bool {
    command.contains(" test")
        || command.contains("cargo nextest")
        || command.contains("pytest")
        || command.contains("vitest")
        || command.contains("jest")
        || command.contains("go test")
}

pub(in crate::agent_os::classification) fn is_harness_command(command: &str) -> bool {
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

pub(in crate::agent_os::classification) fn is_gpu_command(command: &str) -> bool {
    [
        "gpu", "cuda", "nvidia", "vulkan", "wgpu", "directx", "d3d12", "metal",
    ]
    .iter()
    .any(|needle| command.contains(needle))
}

pub(in crate::agent_os::classification) fn is_compile_command(command: &str) -> bool {
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

pub(in crate::agent_os::classification) fn is_run_app_command(command: &str) -> bool {
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

pub(in crate::agent_os::classification) fn is_long_process_command(command: &str) -> bool {
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
