fn cmd_toolbox(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let payload = json!({
        "tool": "c3d-devflywheeltool",
        "codename": "c3d-devflywheeltool",
        "package": "c3d-devflywheeltool",
        "role": "Cunning3D development automation toolbox for reverse engineering, bridge-oracle migration, GPU migration, diagnostics, and future GUI orchestration.",
        "context": {
            "root": ctx.root,
            "tools_gaea": ctx.tools_gaea,
            "gaea_decompiled_root": ctx.gaea_decompiled_root,
            "devflywheel_dir": ctx.devflywheel_dir,
            "artifact_root": ctx.artifact_root,
            "cunning_core_manifest": ctx.cunning_core_manifest,
            "gaea_flywheel_target_dir": ctx.gaea_flywheel_target_dir,
            "cunning_core_target_debug_dir": ctx.cunning_core_target_debug_dir,
            "cunning_core_target_release_dir": ctx.cunning_core_target_release_dir,
        },
        "modules": [
            {
                "name": "gaea_reverse",
                "status": "active",
                "commands": ["reverse", "ledger", "ledger-hygiene", "contracts", "status", "verify"],
                "purpose": "Recover decompiled evidence, classify substrate operators, and guard audited contracts."
            },
            {
                "name": "houdini_sop_oracle",
                "status": "active",
                "commands": ["houdini-fuse-capture"],
                "purpose": "Capture version-pinned Houdini SOP buffers by geometry domain for parity comparisons."
            },
            {
                "name": "houdini_native_reverse",
                "status": "active_seed",
                "commands": ["houdini-native-reverse"],
                "purpose": "Recover bounded native Houdini SOP/GU function evidence through PE exports and Ghidra headless artifacts."
            },
            {
                "name": "bridge_oracle",
                "status": "active",
                "commands": ["certify", "sweep", "raw-gate", "matrix", "capture", "diff", "audit", "canyon-bridge-probe", "canyon-compare", "river-connected-probe", "recurve-bridge-probe"],
                "purpose": "Use Gaea Bridge raw buffers and event traces as the migration truth source."
            },
            {
                "name": "gpu_migration",
                "status": "active_seed",
                "commands": ["raw-gate", "perf-migrate", "gpu-preview", "gpu-sweep", "gpu-candidate-sweep", "gpu-stage-audit", "gpu-substrate", "gpu-wave", "gpu-resident-replay"],
                "purpose": "Compare and classify GPU or hybrid local backend candidates against Bridge with explicit tolerance gates, GPU op profile deltas, and artifacts."
            },
            {
                "name": "gaea_app_perf",
                "status": "active_seed",
                "commands": ["gaea-app-bench"],
                "purpose": "Measure Gaea desktop app or Swarm cook time separately from Bridge correctness timing."
            },
            {
                "name": "gaea_project_harness",
                "status": "active_seed",
                "commands": ["gaea-project"],
                "purpose": "Generate reproducible native Gaea .terrain scenes for node exploration, GUI inspection, and future bridge-oracle migration fixtures."
            },
            {
                "name": "gaea_viewport_reverse",
                "status": "active_seed",
                "commands": ["gaea-viewport-reverse"],
                "purpose": "Reverse and summarize Gaea's Unity viewport DLL, terrain transport, mesh quality tiers, displacement texture upload, and LOD-relevant evidence."
            },
            {
                "name": "gui_orchestration",
                "status": "active_seed",
                "commands": ["graph", "impact", "plan", "export-ui", "blackbox-scan"],
                "purpose": "Native and CLI flywheel atlas views over the same command contracts, blackbox inventory, and artifact roots."
            },
            {
                "name": "reverse_toolchain",
                "status": "active",
                "commands": ["toolchain doctor", "toolchain list", "toolchain sync"],
                "purpose": "Canonical registry and local doctor for Ghidra, ILSpy, Gaea harnesses, native debuggers, shader tools, and reverse evidence utilities."
            }
        ],
        "truth_policy": {
            "gaea_node_migration": "GaeaBridge is the only acceptance oracle.",
            "native_cpu": "Native CPU is a localization helper, not acceptance truth.",
            "gpu_float": "GPU bitwise equality is not required, but coordinate, seed, boundary, branch, and layer semantics cannot be hidden as float error.",
            "performance": "Bridge elapsed time is diagnostic-only. Speed acceptance must compare Cunning Native against measured Gaea desktop app cook time."
        },
        "recommended_next_commands": [
            "/gaea perf-migrate --node Mountain --samples 8 --resolution-choices 128,256 --direct-bin --run --json --gaea-app-baseline-ms <measured_ms> --target-speedup 5",
            "/gaea raw-gate --node Mountain --samples 8 --candidates native_gpu_wave --epsilon 0 --resolution-choices 128,256 --direct-bin --run --json",
            "/gaea gpu-preview --node Mountain --samples 8 --repeat 4 --preview-axis 129 --preview-ms-budget 100 --prewarm --direct-bin --run --json",
            "/gaea gpu-sweep --node Mountain --samples 1 --direct-bin --json",
            "/gaea gpu-candidate-sweep --node Mountain --samples 5 --style-choices basic,old --direct-bin --run --json",
            "/gaea gpu-stage-audit --node Mountain --stage all --direct-bin --run --json",
            "/gaea gpu-substrate --node Mountain --source-resolution 512x384 --target-resolution 128x96 --layers 4 --direct-bin --run --json",
            "/gaea gpu-wave --node Mountain --case old_baseline --epsilon 0.0001 --direct-bin --run --json --max-gpu-cpu-ratio 1.0",
            "/gaea gpu-resident-replay --node Mountain --case old_baseline --resident-wave-count 1 --direct-bin --run --json",
            "/gaea graph --json",
            "/gaea blackbox-scan --json",
            "/gaea toolchain doctor --json",
            "/gaea plan --node Canyon --json",
            "/gaea impact --operator pe --json",
            "/gaea export-ui --json",
            "/gaea gaea-viewport-reverse --run --json",
            "/gaea gpu-sweep --node Mountain --lhs native_gpu_wave --rhs gaea_bridge --seconds 300 --resolution-choices 128,256 --direct-bin --run --json --gaea-app-baseline-ms <measured_ms> --min-gaea-app-speedup 5",
            "/gaea status --node Mountain --json"
        ]
    });
    print_value(cli.json(), &payload);
    Ok(())
}
