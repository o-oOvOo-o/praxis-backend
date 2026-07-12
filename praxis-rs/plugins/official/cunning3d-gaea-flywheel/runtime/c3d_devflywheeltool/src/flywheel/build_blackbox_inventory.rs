
const DEFAULT_ROOT: &str = r"D:\ghost1.0";
const LEDGER_PATH: &str = "ledger/gaea_operator_ledger.json";
const FLYWHEEL_GRAPH_PATH: &str = "ledger/gaea_flywheel_graph.json";
const BLACKBOX_INVENTORY_PATH: &str = "ledger/gaea_blackbox_inventory.json";
const NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH: &str =
    "ledger/gaea_node_performance_acceptance_matrix.json";
const TOOL_COMMAND: &str = "c3d-devflywheeltool";
const DEFAULT_GAEA_FLYWHEEL_TARGET_DIR: &str = r"F:\cargo-target2\Cunning3D_1.0-gaea-flywheel";
const CAPTURE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const MOUNTAIN_GPU_BRIDGE_ORACLE_REMINDER: &str = "Bridge is the only Mountain migration oracle; CPU/GPU resident compares are localizers, not acceptance gates.";
const MOUNTAIN_GPU_ORACLE_VS_CPU_LOCALIZATION: &str = "GaeaBridge raw buffers are the acceptance oracle; native CPU and resident CPU/GPU compares only localize Mountain GPU migration mismatches.";

fn main() {
    let mut cli = Cli::parse(env::args().skip(1).collect()).unwrap_or_else(|error| {
        eprintln!("{error}");
        print_usage();
        std::process::exit(2);
    });

    if cli.command == "help" || cli.command == "--help" || cli.command == "-h" {
        print_usage();
        return;
    }

    let ctx = Context::discover().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    let result = match cli.command.as_str() {
        "toolbox" | "capabilities" => cmd_toolbox(&ctx, &cli),
        "toolchain" | "toolchains" | "reverse-toolchain" => toolchain::cmd_toolchain(&ctx, &cli),
        "toolchain-list" | "reverse-toolchain-list" => toolchain::cmd_toolchain_list(&ctx, &cli),
        "toolchain-doctor" | "reverse-toolchain-doctor" => {
            toolchain::cmd_toolchain_doctor(&ctx, &cli)
        }
        "toolchain-sync" | "reverse-toolchain-sync" => toolchain::cmd_toolchain_sync(&ctx, &cli),
        "reverse" => cmd_reverse(&ctx, &cli),
        "ledger" => cmd_ledger(&ctx, &cli),
        "ledger-hygiene" | "ledger-hygiene-check" => cmd_ledger_hygiene(&ctx, &cli),
        "open-frontier" | "frontier-open" | "open-ledger" => cmd_open_frontier(&ctx, &cli),
        "contracts" => cmd_contracts(&ctx, &cli),
        "status" => cmd_status(&ctx, &cli),
        "praxis-panel" | "plugin-panel" => cmd_praxis_panel(&ctx, &cli),
        "goal-chain-status" | "heightfield-goal-chain" | "chain-status" => {
            cmd_goal_chain_status(&ctx, &cli)
        }
        "goal-chain-bench" | "heightfield-goal-chain-bench" | "chain-bench" => {
            cmd_goal_chain_bench(&ctx, &cli)
        }
        "acceptance-matrix" | "performance-acceptance" | "speed-acceptance" => {
            cmd_acceptance_matrix(&ctx, &cli)
        }
        "frontier-health" | "gaea-frontier-health" | "frontier-probes" => {
            cmd_frontier_health(&ctx, &cli)
        }
        "graph" | "atlas" | "flywheel-graph" => cmd_flywheel_graph(&ctx, &cli),
        "impact" | "flywheel-impact" => cmd_flywheel_impact(&ctx, &cli),
        "plan" | "flywheel-plan" => cmd_flywheel_plan(&ctx, &cli),
        "export-ui" | "ui-graph" | "flywheel-ui" => cmd_flywheel_export_ui(&ctx, &cli),
        "blackbox-scan" | "scan-blackboxes" | "blackbox-inventory" => cmd_blackbox_scan(&ctx, &cli),
        "verify" => cmd_verify(&ctx, &cli),
        "certify" => cmd_certify(&ctx, &cli),
        "sweep" => cmd_sweep(&ctx, &cli),
        "raw-gate" | "mountain-raw-gate" | "gpu-raw-gate" => cmd_raw_gate(&ctx, &cli),
        "gaea-project" | "gaea-scene" => cmd_gaea_project(&ctx, &cli),
        "gaea-viewport-reverse" | "viewport-reverse" => cmd_gaea_viewport_reverse(&ctx, &cli),
        "gaea-app-bench" | "app-bench" => cmd_gaea_app_bench(&ctx, &cli),
        "perf-migrate" | "speed-migrate" => cmd_perf_migrate(&ctx, &cli),
        "gpu-sweep" => cmd_gpu_sweep(&ctx, &cli),
        "gpu-preview" | "gpu-preview-sweep" => cmd_gpu_preview(&ctx, &cli),
        "gpu-candidate-sweep" => cmd_gpu_candidate_sweep(&ctx, &cli),
        "gpu-stage-audit" => cmd_gpu_stage_audit(&ctx, &cli),
        "gpu-substrate" => cmd_gpu_substrate(&ctx, &cli),
        "gpu-wave" | "gpu-wave-writeback" => cmd_gpu_wave(&ctx, &cli),
        "gpu-resident-replay" => cmd_gpu_resident_replay(&ctx, &cli),
        "heightfield-art-status" | "hf-art-status" | "art-node-status" => {
            cmd_heightfield_art_status(&ctx, &cli)
        }
        "heightfield-art-gaea-baseline" | "hf-art-gaea-baseline" | "art-gaea-baseline" => {
            cmd_heightfield_art_gaea_baseline(&ctx, &cli)
        }
        "mountain-display-log-audit" | "mountain-display-audit" | "mountain-render-log-audit" => {
            cmd_mountain_display_log_audit(&ctx, &cli)
        }
        "live-heightfield-audit" | "live-hf-audit" | "heightfield-live-audit" => {
            cmd_live_heightfield_audit(&ctx, &cli)
        }
        "river-connected-probe" | "rivers-connected-probe" => cmd_river_connected_probe(&ctx, &cli),
        "mask-flow-mountain-connected-probe"
        | "gradient-slope-mask-mountain-connected-probe"
        | "mountain-mask-flow-connected-probe" => {
            cmd_mask_flow_mountain_connected_probe(&ctx, &cli)
        }
        "canyon-bridge-probe" | "canyon-probe" => cmd_canyon_bridge_probe(&ctx, &cli),
        "canyon-compare" | "canyon-bridge-native-compare" => cmd_canyon_compare(&ctx, &cli),
        "easy-erosion-compare"
        | "easy-erosion-bridge-native-compare"
        | "easy-erosion-bridge-probe"
        | "easy-erosion-probe"
        | "easyerosion-compare" => cmd_easy_erosion_compare(&ctx, &cli),
        "mountain-side-compare"
        | "mountainside-compare"
        | "mountain-side-bridge-native-compare" => cmd_mountain_side_compare(&ctx, &cli),
        "stratify-compare" | "stratify-bridge-native-compare" => cmd_stratify_compare(&ctx, &cli),
        "fractal-terrace-internals"
        | "fractal-terraces-internals"
        | "fractal-terrace-internal-compare"
        | "fractal-terraces-internal-compare" => cmd_fractal_terrace_internals(&ctx, &cli),
        "fractal-terraces-bridge-probe"
        | "fractal-terrace-bridge-probe"
        | "fractal-terraces-runtime-bridge" => cmd_fractal_terraces_bridge_probe(&ctx, &cli),
        "terraces-compare" | "terraces-bridge-native-compare" => cmd_terraces_compare(&ctx, &cli),
        "ridge-compare" | "ridge-bridge-native-compare" => cmd_ridge_compare(&ctx, &cli),
        "slump-compare" | "slump-bridge-native-compare" | "slump-stage-compare" => {
            cmd_slump_compare(&ctx, &cli)
        }
        "stones-compare" | "stones-bridge-native-compare" => cmd_stones_compare(&ctx, &cli),
        "scree-compare" | "scree-bridge-native-compare" => cmd_scree_compare(&ctx, &cli),
        "rock-core-compare"
        | "rockcore-compare"
        | "rock-core-static-oracle-compare"
        | "outcrops-compare"
        | "outcrops-bridge-native-compare" => cmd_rock_core_compare(&ctx, &cli),
        "rock-noise-compare" | "rocknoise-compare" | "rock-noise-bridge-native-compare" => {
            cmd_rock_noise_compare(&ctx, &cli)
        }
        "rugged-stage-compare"
        | "rugged-stages-compare"
        | "rugged-stage-bridge-native-compare"
        | "rugged-m3-stage-compare" => cmd_rugged_stage_compare(&ctx, &cli),
        "combiner-mountain-connected-probe"
        | "combine-mountain-connected-probe"
        | "mountain-combiner-connected-probe" => cmd_combiner_mountain_connected_probe(&ctx, &cli),
        "combiner-compare" | "combiner-bridge-native-compare" => cmd_combiner_compare(&ctx, &cli),
        "slope-warp-compare" | "slope-warp-bridge-native-compare" => {
            cmd_slope_warp_compare(&ctx, &cli)
        }
        "thermal-shaper-compare" | "thermal-shaper-bridge-native-compare" => {
            cmd_thermal_shaper_compare(&ctx, &cli)
        }
        "thermal2-compare" | "thermal2-bridge-native-compare" => cmd_thermal2_compare(&ctx, &cli),
        "thermal2-bridge-probe" | "thermal2-probe" => cmd_thermal2_bridge_probe(&ctx, &cli),
        "directional-warp-compare" | "directional-warp-bridge-native-compare" => {
            cmd_directional_warp_compare(&ctx, &cli)
        }
        "warp-compare" | "warp-bridge-native-compare" => cmd_warp_compare(&ctx, &cli),
        "erosion2-inhibitor-probe" | "erosion2-inhibitor-compare" => {
            cmd_erosion2_inhibitor_probe(&ctx, &cli)
        }
        "erosion-classic-bridge-probe"
        | "erosion-classic-probe"
        | "erosion-bridge-probe"
        | "classic-erosion-bridge-probe" => cmd_erosion_classic_bridge_probe(&ctx, &cli),
        "erosion-classic-substrate-compare"
        | "classic-erosion-substrate-compare"
        | "erosion-substrate-compare" => cmd_erosion_classic_substrate_compare(&ctx, &cli),
        "erosion2-compare" | "erosion2-bridge-native-compare" => cmd_erosion2_compare(&ctx, &cli),
        "sand-compare" | "sand-bridge-native-compare" => cmd_sand_compare(&ctx, &cli),
        "crater-compare" | "crater-bridge-native-compare" => cmd_crater_compare(&ctx, &cli),
        "craterfield-compare" | "craterfield-bridge-native-compare" => {
            cmd_craterfield_compare(&ctx, &cli)
        }
        "crumble-compare" | "crumble-bridge-native-compare" => {
            crumble::cmd_crumble_compare(&ctx, &cli)
        }
        "debris-compare" | "debris-backend-compare" => debris::cmd_debris_compare(&ctx, &cli),
        "transform-compare" | "transform-bridge-mountain-compare" => {
            cmd_transform_compare(&ctx, &cli)
        }
        "recurve-bridge-probe" | "recurve-probe" => cmd_recurve_bridge_probe(&ctx, &cli),
        "blur-bridge-probe" | "blur-probe" | "gaea-blur-bridge-probe" => {
            cmd_blur_bridge_probe(&ctx, &cli)
        }
        "graphic-eq-bridge-probe" | "graphic-eq-probe" | "graphiceq-probe" => {
            cmd_graphic_eq_bridge_probe(&ctx, &cli)
        }
        "deflate-bridge-probe" | "deflate-probe" => cmd_deflate_bridge_probe(&ctx, &cli),
        "denoise-bridge-probe" | "denoise-probe" => cmd_denoise_bridge_probe(&ctx, &cli),
        "peaks-bridge-probe" | "peaks-probe" => cmd_peaks_bridge_probe(&ctx, &cli),
        "uplift-bridge-probe" | "uplift-probe" => cmd_uplift_bridge_probe(&ctx, &cli),
        "weathering-probe" | "weathering-native-probe" => cmd_weathering_native_probe(&ctx, &cli),
        "dune-sea-probe" | "dune-sea-native-probe" => cmd_dune_sea_native_probe(&ctx, &cli),
        "dune-sea-compare" | "dune-sea-bridge-native-compare" => cmd_dune_sea_compare(&ctx, &cli),
        "flow-map-classic-compare" | "flow-classic-compare" | "flowmapclassic-compare" => {
            cmd_flow_map_classic_compare(&ctx, &cli)
        }
        "sharpen-bridge-probe" | "sharpen-probe" => cmd_sharpen_bridge_probe(&ctx, &cli),
        "gabor-bridge-probe" | "gabor-probe" => cmd_gabor_bridge_probe(&ctx, &cli),
        "distress-bridge-probe"
        | "distress-probe"
        | "distress-compare"
        | "distress-bridge-native-compare" => cmd_distress_bridge_probe(&ctx, &cli),
        "sea-bridge-probe" | "sea-probe" => cmd_sea_bridge_probe(&ctx, &cli),
        "flow-map-bridge-probe" | "flowmap-bridge-probe" | "flow-map-probe" | "flowmap-probe" => {
            cmd_flow_map_bridge_probe(&ctx, &cli)
        }
        "cracks-bridge-probe"
        | "cracks-probe"
        | "cracks-compare"
        | "cracks-bridge-native-compare" => cmd_cracks_bridge_probe(&ctx, &cli),
        "distance-bridge-probe"
        | "distance-probe"
        | "distance-compare"
        | "distance-bridge-native-compare" => cmd_distance_bridge_probe(&ctx, &cli),
        "plates-bridge-probe"
        | "plates-probe"
        | "plates-compare"
        | "plates-bridge-native-compare" => cmd_plates_bridge_probe(&ctx, &cli),
        "lake-bridge-probe" | "lake-probe" => cmd_lake_bridge_probe(&ctx, &cli),
        "hydro-fix-bridge-probe"
        | "hydrofix-bridge-probe"
        | "hydro-fix-probe"
        | "hydrofix-probe" => cmd_hydro_fix_bridge_probe(&ctx, &cli),
        "snow-bridge-probe"
        | "snow-probe"
        | "snow-mountain-connected-probe"
        | "snow-connected-mountain-probe" => cmd_snow_bridge_probe(&ctx, &cli),
        "snowfield-bridge-probe" | "snowfield-probe" => cmd_snowfield_bridge_probe(&ctx, &cli),
        "glacier-bridge-probe" | "glacier-probe" => cmd_glacier_bridge_probe(&ctx, &cli),
        "aspect-bridge-probe"
        | "aspect-probe"
        | "height-bridge-probe"
        | "height-probe"
        | "slope-bridge-probe"
        | "slope-probe"
        | "angle-bridge-probe"
        | "angle-probe"
        | "curvature-bridge-probe"
        | "curvature-probe" => cmd_aspect_bridge_probe(&ctx, &cli),
        "gradient-bridge-probe" | "linear-gradient-bridge-probe" | "linear-gradient-probe" => {
            cmd_mask_flow_bridge_probe(
                &ctx,
                &cli,
                "gradient-bridge-probe",
                "LinearGradient",
                &[
                    "LinearGradient",
                    "Gradient",
                    "Gradients.LinearGradient",
                    "RadialGradient",
                    "Gradients.RadialGradient",
                    "Cone",
                    "Gradients.Cone",
                    "Hemisphere",
                    "Dome",
                ],
            )
        }
        "radial-gradient-bridge-probe" | "radial-gradient-probe" => cmd_mask_flow_bridge_probe(
            &ctx,
            &cli,
            "radial-gradient-bridge-probe",
            "RadialGradient",
            &["RadialGradient", "Gradients.RadialGradient"],
        ),
        "cone-bridge-probe" | "cone-probe" => cmd_mask_flow_bridge_probe(
            &ctx,
            &cli,
            "cone-bridge-probe",
            "Cone",
            &["Cone", "Gradients.Cone"],
        ),
        "hemisphere-bridge-probe" | "hemisphere-probe" | "dome-bridge-probe" => {
            cmd_mask_flow_bridge_probe(
                &ctx,
                &cli,
                "hemisphere-bridge-probe",
                "Hemisphere",
                &["Hemisphere", "Dome", "HemisphereProcess"],
            )
        }
        "slope-mask-bridge-probe" | "slope-mask-probe" | "modifier-slope-bridge-probe" => {
            cmd_mask_flow_bridge_probe(
                &ctx,
                &cli,
                "slope-mask-bridge-probe",
                "SlopeMask",
                &["SlopeMask", "ModifierSlope", "SlopeFlow"],
            )
        }
        "mask-bridge-probe" | "mask-probe" | "masking-bridge-probe" => cmd_mask_flow_bridge_probe(
            &ctx,
            &cli,
            "mask-bridge-probe",
            "Mask",
            &["Mask", "Masking.Mask", "MaskingMask"],
        ),
        "ground-texture-bridge-probe"
        | "groundtexture-bridge-probe"
        | "ground-texture-probe"
        | "groundtexture-probe" => cmd_ground_texture_bridge_probe(&ctx, &cli),
        "volcano-stage-parity" | "volcano-parity" | "volcano-stage-matrix" => {
            cmd_volcano_stage_parity(&ctx, &cli)
        }
        "island-process-probe" | "island-probe" => cmd_island_process_probe(&ctx, &cli),
        "island-process-sweep" | "island-sweep" => cmd_island_process_sweep(&ctx, &cli),
        "probe-bin" | "run-probe" | "gaea-probe" | "isolated-probe" => cmd_probe_bin(&ctx, &cli),
        "matrix" => cmd_matrix(&ctx, &cli),
        "capture" => cmd_capture(&ctx, &mut cli),
        "diff" => cmd_diff(&ctx, &mut cli),
        "audit" => cmd_audit(&ctx, &mut cli),
        other => Err(format!("Unknown command '{other}'.")),
    };

    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct Cli {
    command: String,
    flags: BTreeMap<String, Vec<String>>,
    passthrough: Vec<String>,
}

impl Cli {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        if args.is_empty() {
            return Ok(Self {
                command: "help".to_string(),
                flags: BTreeMap::new(),
                passthrough: Vec::new(),
            });
        }
        let command = args[0].clone();
        let mut flags: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut passthrough = Vec::new();
        let mut index = 1usize;
        if matches!(
            command.as_str(),
            "toolchain" | "toolchains" | "reverse-toolchain"
        ) && args
            .get(1)
            .map(|arg| !arg.starts_with("--"))
            .unwrap_or(false)
        {
            flags
                .entry("mode".to_string())
                .or_default()
                .push(args[1].clone());
            index = 2;
        }
        while index < args.len() {
            let arg = &args[index];
            if arg == "--" {
                passthrough.extend(args[index + 1..].iter().cloned());
                break;
            }
            if !arg.starts_with("--") {
                return Err(format!("Unexpected positional argument '{arg}'."));
            }
            let key = arg.trim_start_matches("--").to_string();
            let takes_value = !matches!(
                key.as_str(),
                "json"
                    | "run"
                    | "dry-run"
                    | "stage-report"
                    | "classic-stage-report"
                    | "shadow-focused"
                    | "first"
                    | "all"
                    | "help"
                    | "require-exact"
                    | "require-accepted"
                    | "native-only"
                    | "profile-native"
                    | "compare-native"
                    | "compare-stages"
                    | "compare-bridge"
                    | "ao-only"
                    | "ao-timing-only"
                    | "ao-root-replay-only"
                    | "ao-focused-raw-photon-only"
                    | "ao-normal-z-scale-sweep"
                    | "deep"
                    | "direct-bin"
                    | "release-bin"
                    | "no-incremental"
                    | "fresh-bridge-cache"
                    | "allow-stale-direct-bin"
                    | "file-capture"
                    | "keep-going"
                    | "worst-cell-diagnostics"
                    | "aux-diagnostics"
                    | "profile"
                    | "skip-native-preflight"
                    | "skip-seed-packets"
                    | "seed-packets-only"
                    | "require-all-pass"
                    | "require-consistent"
                    | "require-finite"
                    | "require-performance"
                    | "require-goal-complete"
                    | "require-pass"
                    | "require-speedup"
                    | "require-gaea-speedup"
                    | "require-bridge-exact"
                    | "require-speedup-gate"
                    | "capture-live-stages"
                    | "dump-stages"
                    | "require-gpu-active"
                    | "gpu-exact-barrier"
                    | "trace-probe"
                    | "trace-directions"
                    | "path-commit-scalar-focus"
                    | "path-commit-integrated-debug"
                    | "cpu-trace-barrier"
                    | "cpu-commit-barrier"
                    | "resident-break-on-inactive"
                    | "resident-wave-loop"
                    | "resident-layer-loop"
                    | "resident-layer-cpu-shape-loop"
                    | "force-gpu-wave"
                    | "prewarm"
                    | "open"
                    | "no-new-console"
                    | "verbose"
                    | "offline"
                    | "repair"
                    | "strict"
                    | "include-traces"
                    | "include-optional"
                    | "include-pixels"
                    | "inverse"
                    | "darker"
                    | "verify-gpu"
                    | "gpu"
                    | "verify-handle-gpu"
                    | "handle-gpu"
                    | "keep-nodes"
                    | "render-still-rocks"
                    | "debris-point-cloud"
                    | "debris-export-point-cloud"
            );
            if takes_value {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("--{key} requires a value."))?
                    .clone();
                flags.entry(key).or_default().push(value);
            } else {
                flags.entry(key).or_default().push("true".to_string());
            }
            index += 1;
        }
        Ok(Self {
            command,
            flags,
            passthrough,
        })
    }

    fn flag(&self, key: &str) -> Option<&str> {
        self.flags
            .get(key)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    fn has(&self, key: &str) -> bool {
        self.flags.contains_key(key)
    }

    fn prefers_release_probe_bins(&self) -> bool {
        self.has("release-bin") || matches!(self.command.as_str(), "perf-migrate" | "speed-migrate")
    }

    fn node(&self) -> String {
        self.flag("node").unwrap_or("Mountain").to_string()
    }

    fn case_name(&self) -> String {
        self.flag("case").unwrap_or("old_baseline").to_string()
    }

    fn json(&self) -> bool {
        self.has("json")
    }

    fn run(&self) -> bool {
        self.has("run") && !self.has("dry-run")
    }
}

#[derive(Debug)]
struct Context {
    root: PathBuf,
    tools_gaea: PathBuf,
    summary_dir: PathBuf,
    harness_project: PathBuf,
    harness_exe: PathBuf,
    cunning_core_manifest: PathBuf,
    gaea_flywheel_target_dir: PathBuf,
    cunning_core_target_debug_dir: PathBuf,
    cunning_core_target_release_dir: PathBuf,
    devflywheel_dir: PathBuf,
    artifact_root: PathBuf,
}

impl Context {
    fn discover() -> Result<Self, String> {
        let root = env::var_os("GHOST1_ROOT")
            .map(PathBuf::from)
            .and_then(normalize_ghost1_root)
            .or_else(|| {
                env::var_os("CUNNING3D_ROOT")
                    .map(PathBuf::from)
                    .and_then(normalize_ghost1_root)
            })
            .or_else(find_root_from_current_dir)
            .or_else(|| {
                let candidate = PathBuf::from(DEFAULT_ROOT);
                normalize_ghost1_root(candidate)
            })
            .ok_or_else(|| {
                "Could not discover ghost1 root. Run from D:\\ghost1.0\\Cunning3D_1.0 or set GHOST1_ROOT.".to_string()
            })?;
        let tools_gaea = root.join("tools").join("gaea");
        let summary_dir = root.join("_gaea_decompiled").join("_summary");
        let harness_project = tools_gaea
            .join("GaeaReverseHarness")
            .join("GaeaReverseHarness.csproj");
        let harness_exe = tools_gaea
            .join("GaeaReverseHarness")
            .join("bin")
            .join("Debug")
            .join("net8.0-windows")
            .join("GaeaReverseHarness.exe");
        let cunning_core_manifest = root
            .join("Cunning3D_1.0")
            .join("crates")
            .join("cunning_core")
            .join("Cargo.toml");
        let gaea_flywheel_target_dir = gaea_flywheel_target_dir();
        let cunning_core_target_debug_dir = gaea_flywheel_target_dir.join("debug");
        let cunning_core_target_release_dir = gaea_flywheel_target_dir.join("release");
        let devflywheel_dir = discover_devflywheel_dir(&root)?;
        let artifact_root = env::var_os("C3D_DEVFLYWHEEL_ARTIFACT_ROOT")
            .or_else(|| env::var_os("GHOST1_DEVFLYWHEEL_ARTIFACT_ROOT"))
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("_c3d_devflywheeltool"));
        Ok(Self {
            root,
            tools_gaea,
            summary_dir,
            harness_project,
            harness_exe,
            cunning_core_manifest,
            gaea_flywheel_target_dir,
            cunning_core_target_debug_dir,
            cunning_core_target_release_dir,
            devflywheel_dir,
            artifact_root,
        })
    }
}

fn find_root_from_current_dir() -> Option<PathBuf> {
    let current = env::current_dir().ok()?;
    for dir in current.ancestors() {
        if let Some(root) = normalize_ghost1_root(dir.to_path_buf()) {
            return Some(root);
        }
    }
    None
}

fn normalize_ghost1_root(candidate: PathBuf) -> Option<PathBuf> {
    if candidate
        .join("Cunning3D_1.0")
        .join("crates")
        .join("cunning_core")
        .join("Cargo.toml")
        .is_file()
    {
        return Some(candidate);
    }
    if candidate
        .join("crates")
        .join("cunning_core")
        .join("Cargo.toml")
        .is_file()
    {
        return candidate.parent().map(Path::to_path_buf);
    }
    None
}

fn discover_devflywheel_dir(_root: &Path) -> Result<PathBuf, String> {
    let dir = env::var_os("C3D_DEVFLYWHEEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if dir.join("Cargo.toml").exists() && dir.join(LEDGER_PATH).exists() {
        Ok(dir)
    } else {
        Err(format!(
            "The Praxis Gaea flywheel runtime is incomplete at '{}'. Reinstall the cunning3d-gaea-flywheel plugin or set C3D_DEVFLYWHEEL_DIR.",
            dir.display()
        ))
    }
}

fn gaea_flywheel_target_dir() -> PathBuf {
    env::var_os("C3D_GAEA_FLYWHEEL_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAEA_FLYWHEEL_TARGET_DIR))
}

fn cmd_toolbox(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let payload = json!({
        "tool": "c3d-devflywheeltool",
        "codename": "c3d-devflywheeltool",
        "package": "c3d-devflywheeltool",
        "role": "Cunning3D development automation toolbox for reverse engineering, bridge-oracle migration, GPU migration, diagnostics, and future GUI orchestration.",
        "context": {
            "root": ctx.root,
            "tools_gaea": ctx.tools_gaea,
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

#[derive(Debug, Deserialize)]
struct DossierIndex {
    gaea_dir: Option<String>,
    #[serde(default)]
    seeded_node_dossiers: BTreeMap<String, String>,
    #[serde(default)]
    seeded_owner_dossiers: BTreeMap<String, String>,
    #[serde(default)]
    seeded_kernel_dossiers: BTreeMap<String, String>,
}

#[derive(Debug)]
struct CoverageRow {
    values: BTreeMap<String, String>,
}

fn cmd_reverse(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let index_path = ctx.summary_dir.join("gaea_dossier_index.json");
    let index: DossierIndex = read_json(&index_path)?;
    let coverage = read_coverage(&ctx.summary_dir.join("gaea_reverse_coverage.tsv"))?;
    let coverage_row = coverage
        .iter()
        .find(|row| row.get("node").eq_ignore_ascii_case(&node));
    let dossier = resolve_dossier(&index, coverage_row, &node);
    let evidence = coverage_row
        .and_then(|row| row.values.get("evidence").cloned())
        .unwrap_or_default();
    let unresolved = coverage_row
        .and_then(|row| row.values.get("unresolved").cloned())
        .unwrap_or_default();
    let related_files = find_related_summary_files(&ctx.summary_dir, &node, dossier.as_deref())?;
    let node_surface_contract = node_surface_contract(ctx, &node);
    let payload = json!({
        "node": node,
        "context": {
            "root": ctx.root,
            "tools_gaea": ctx.tools_gaea,
            "harness_project": ctx.harness_project,
            "harness_exe": ctx.harness_exe,
            "cunning_core_manifest": ctx.cunning_core_manifest,
            "gaea_flywheel_target_dir": ctx.gaea_flywheel_target_dir,
            "cunning_core_target_debug_dir": ctx.cunning_core_target_debug_dir,
            "cunning_core_target_release_dir": ctx.cunning_core_target_release_dir,
        },
        "gaea_dir": index.gaea_dir,
        "index_counts": {
            "node_dossiers": index.seeded_node_dossiers.len(),
            "owner_dossiers": index.seeded_owner_dossiers.len(),
            "kernel_dossiers": index.seeded_kernel_dossiers.len(),
        },
        "dossier": dossier.as_ref().map(|name| ctx.summary_dir.join(name).display().to_string()),
        "coverage": coverage_row.map(|row| &row.values),
        "evidence_tokens": split_semicolon_list(&evidence),
        "unresolved": unresolved,
        "related_summary_files": related_files,
        "node_surface_contract": node_surface_contract,
        "closure_gates": gaea_node_closure_gates(),
        "recommended_next_commands": reverse_recommendations(&node),
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn gaea_node_closure_gates() -> Value {
    json!({
        "raw_buffer_parity": "Bridge/native raw buffers must pass the agreed exact or epsilon gate for every promoted scope.",
        "parameter_surface_parity": "Parameter names, defaults, ranges, UI types, command buttons, hidden state, and visibility conditions must be copied from decompiled Gaea evidence before claiming node parity.",
        "port_surface_parity": "Input/output ports must be derived from constructor ports, base.In/base.Ins usage, AddNewPort, CanCreatePorts, port Order, named lookups, and Build loops; do not infer port count from generated C3D project fixtures.",
        "constant_decode_rule": "Obfuscated constants such as \\ue0003.\\ue000(N) are unresolved until proven by runtime reflection or contextual callsite evidence; never treat one generated .terrain value as stronger than decompiled source behavior."
    })
}

fn node_surface_contract(ctx: &Context, node: &str) -> Value {
    let Some(source_path) = find_decompiled_node_source(ctx, node) else {
        return json!({
            "status": "source_not_found",
            "source_authority": "Unavailable. Do not close parameter or port parity from raw buffers alone.",
            "checklist": node_surface_checklist(),
        });
    };
    let Ok(text) = fs::read_to_string(&source_path) else {
        return json!({
            "status": "source_unreadable",
            "source": source_path,
            "source_authority": "Unreadable source. Do not close parameter or port parity from raw buffers alone.",
            "checklist": node_surface_checklist(),
        });
    };
    json!({
        "status": "source_scanned",
        "source": source_path,
        "source_authority": "Decompiled Gaea node source is the authority for UI parameters and ports; generated .terrain or C3D fixture files are secondary evidence.",
        "class_and_attribute_evidence": matching_source_lines(&text, &[
            "[Name(",
            "[Family(",
            "[Classification(",
            "[CanCreatePorts(",
            " class ",
        ], 24),
        "parameter_surface_evidence": matching_source_lines(&text, &[
            "[Parameter",
            "<PortCount>",
            "VisibilityTable",
            "SwitchInputs",
            "AddInput",
            "ProcessInput",
            "ClampType",
            "BlendMode",
        ], 64),
        "port_surface_evidence": matching_source_lines(&text, &[
            "base.Ports.Add",
            "new Port(",
            "Order =",
            "AddNewPort",
            "PortCount",
            "base.In",
            "base.Ins",
            "Mask",
            "Commit(",
        ], 96),
        "dynamic_port_risk": text.contains("AddNewPort") || text.contains("[CanCreatePorts("),
        "obfuscated_constants_present": has_obfuscated_constants(&text),
        "checklist": node_surface_checklist(),
    })
}

fn node_surface_checklist() -> Vec<&'static str> {
    vec![
        "List every [Parameter] attribute with default, range, UI kind, display name, and command semantics.",
        "List hidden backing state such as PortCount and prove each obfuscated default before implementation.",
        "List constructor-created ports separately from base.In and output ports.",
        "Trace Build input loops and named port skips before assigning slot names.",
        "Trace AddNewPort and CanCreatePorts before deciding max dynamic input count.",
        "Add a focused test that asserts Cunning3D node parameter names and port names/counts match the recovered surface.",
    ]
}

fn find_decompiled_node_source(ctx: &Context, node: &str) -> Option<PathBuf> {
    let roots = [
        ctx.root.join("_gaea_decompiled").join("Gaea.Nodes"),
        ctx.root.join("_gaea_decompiled").join("Gaea"),
    ];
    let mut candidates = Vec::new();
    for root in roots {
        collect_cs_files(&root, &mut candidates);
    }
    let node_lower = node.to_ascii_lowercase();
    for path in &candidates {
        if !path_file_stem_matches(path, &node_lower) {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_has_exact_name_attribute(&text, node) {
            return Some(path.clone());
        }
    }
    for path in &candidates {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_has_exact_name_attribute(&text, node) {
            return Some(path.clone());
        }
    }
    for path in &candidates {
        if !path_file_stem_matches(path, &node_lower) {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_declares_exact_node_class(&text, node) {
            return Some(path.clone());
        }
    }
    for path in &candidates {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_declares_exact_node_class(&text, node) {
            return Some(path.clone());
        }
    }
    candidates
        .into_iter()
        .find(|path| path_file_stem_matches(path, &node_lower))
}

fn path_file_stem_matches(path: &Path, node_lower: &str) -> bool {
    path.file_stem()
        .and_then(OsStr::to_str)
        .map(|stem| stem.eq_ignore_ascii_case(node_lower))
        .unwrap_or(false)
}

fn source_has_exact_name_attribute(text: &str, node: &str) -> bool {
    let exact_name = format!("[Name(\"{node}\")]");
    text.contains(&exact_name)
}

fn source_declares_exact_node_class(text: &str, node: &str) -> bool {
    text.lines()
        .any(|line| line_declares_exact_node_class(line, node))
}

fn line_declares_exact_node_class(line: &str, node: &str) -> bool {
    let mut tokens = line
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty());
    while let Some(token) = tokens.next() {
        if token == "class" {
            return tokens
                .next()
                .map(|class_name| class_name.eq_ignore_ascii_case(node))
                .unwrap_or(false);
        }
    }
    false
}

fn collect_cs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cs_files(&path, files);
        } else if path.extension().and_then(OsStr::to_str) == Some("cs") {
            files.push(path);
        }
    }
}

fn matching_source_lines(text: &str, patterns: &[&str], max_count: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if patterns.iter().any(|pattern| trimmed.contains(pattern)) {
            lines.push(format!("{}: {}", line_index + 1, trimmed));
            if lines.len() >= max_count {
                break;
            }
        }
    }
    lines
}

fn has_obfuscated_constants(text: &str) -> bool {
    text.contains("\\ue")
        || text
            .chars()
            .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch))
}

fn cmd_gaea_viewport_reverse(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let gaea_dir = cli
        .flag("gaea-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_gaea_install_dir);
    let managed_dir = gaea_dir.join("Gaea.Viewport_Data").join("Managed");
    let viewport_dll = managed_dir.join("Assembly-CSharp.dll");
    let main_comms = ctx
        .root
        .join("_gaea_decompiled")
        .join("Gaea")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Comms.cs");
    let main_b = ctx
        .root
        .join("_gaea_decompiled")
        .join("Gaea")
        .join("QuadSpinner")
        .join("Gaea")
        .join("B.cs");
    let viewport_area = ctx
        .root
        .join("_gaea_decompiled")
        .join("Gaea")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Areas")
        .join("ViewportArea.cs");
    let command = gaea_viewport_reverse_command(&gaea_dir);
    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "gaea-viewport-reverse",
                "gaea_dir": gaea_dir,
                "viewport_dll": viewport_dll,
                "main_source_evidence_paths": [main_comms, main_b, viewport_area],
                "command_preview": command_preview(&command),
                "note": "Pass --run to reflect/decompile the Gaea Unity viewport DLL and write an artifact."
            }),
        );
        return Ok(());
    }
    if !viewport_dll.exists() {
        return Err(format!(
            "Gaea viewport DLL not found at '{}'. Pass --gaea-dir <path>.",
            viewport_dll.display()
        ));
    }
    let run_dir = ctx
        .artifact_root
        .join("gaea_viewport_reverse")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let raw_stdout_path = run_dir.join("viewport_reflection_stdout.json");
    let stderr_path = run_dir.join("viewport_reflection_stderr.txt");
    write_text(&raw_stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let reflected = serde_json::from_str::<Value>(&stdout_text).map_err(|error| {
        format!(
            "Failed to parse viewport reflection JSON '{}': {error}",
            raw_stdout_path.display()
        )
    })?;
    let main_source_evidence =
        gaea_viewport_main_source_evidence(&main_comms, &main_b, &viewport_area);
    let conclusion = gaea_viewport_conclusion(&reflected);
    let payload = json!({
        "mode": "executed",
        "command": "gaea-viewport-reverse",
        "artifact_dir": run_dir,
        "status": output.status_code,
        "gaea_dir": gaea_dir,
        "viewport_dll": viewport_dll,
        "raw_stdout": raw_stdout_path,
        "stderr": stderr_path,
        "conclusion": conclusion,
        "main_source_evidence": main_source_evidence,
        "viewport_reflection": reflected,
        "cunning_viewport_direction": {
            "display_contract": "Keep full-resolution height texture data and reduce viewport geometry separately.",
            "gaea_like_path": "Upload height as a texture, render fixed quality-tier plane mesh, displace in material/shader, and avoid rebuilding full-resolution CPU mesh for viewport display.",
            "not_supported_by_evidence": "Unity TerrainData/SetHeights/quadtree terrain LOD is not referenced by Assembly-CSharp.dll metadata."
        }
    });
    let summary_path = run_dir.join("gaea_viewport_reverse_summary.json");
    let report_path = run_dir.join("gaea_viewport_reverse_report.md");
    write_pretty_json(&summary_path, &payload)?;
    write_text(&report_path, &gaea_viewport_report_markdown(&payload))?;
    print_value(cli.json(), &payload);
    if output.status_code != 0 {
        return Err(format!(
            "Gaea viewport reverse command failed with status {}. See '{}'.",
            output.status_code,
            stderr_path.display()
        ));
    }
    Ok(())
}

fn resolve_dossier(index: &DossierIndex, row: Option<&CoverageRow>, node: &str) -> Option<String> {
    index
        .seeded_node_dossiers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(node))
        .map(|(_, value)| value.clone())
        .or_else(|| {
            row.and_then(|row| {
                let dossier = row.values.get("dossier")?;
                (!dossier.trim().is_empty()).then_some(dossier.clone())
            })
        })
}

fn reverse_recommendations(node: &str) -> Vec<String> {
    let node_lower = node.to_ascii_lowercase();
    if node_lower == "mountain" {
        vec![
            format!("{TOOL_COMMAND} ledger --operator pe_commit.capacity_with_extra"),
            format!("{TOOL_COMMAND} diff --node Mountain --case old_baseline --first --run"),
            format!("{TOOL_COMMAND} audit --node Mountain --case all --run"),
        ]
    } else {
        vec![
            format!("{TOOL_COMMAND} capture --node {node} --case baseline"),
            format!("{TOOL_COMMAND} diff --node {node} --case baseline --first"),
            format!("{TOOL_COMMAND} ledger --node {node} --all"),
        ]
    }
}

#[derive(Debug, Deserialize)]
struct Ledger {
    schema_version: u32,
    entries: Vec<LedgerEntry>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct LedgerEntry {
    operator: String,
    node: String,
    layer: String,
    status: String,
    native_evidence: Vec<String>,
    rust_implementation: Vec<String>,
    evidence_summary: String,
    open_risk: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelGraph {
    schema_version: u32,
    contracts: Vec<FlywheelContract>,
    nodes: Vec<FlywheelNode>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelContract {
    id: String,
    label: String,
    kind: String,
    layer: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    ledger_operators: Vec<String>,
    #[serde(default)]
    owner_nodes: Vec<String>,
    #[serde(default)]
    reusable: bool,
    #[serde(default)]
    unlocks: Vec<String>,
    #[serde(default)]
    implementation: Vec<String>,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    next_commands: Vec<String>,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelNode {
    id: String,
    label: String,
    domain: String,
    kind: String,
    priority: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    input_ports: Vec<FlywheelPort>,
    #[serde(default)]
    output_ports: Vec<FlywheelPort>,
    #[serde(default)]
    shared_operators: Vec<String>,
    #[serde(default)]
    recipe_families: Vec<String>,
    #[serde(default)]
    next_commands: Vec<String>,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FlywheelPort {
    name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    slot: Option<usize>,
    #[serde(default)]
    source_slot: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxInventory {
    schema_version: u32,
    generated_by: String,
    generated_from: String,
    node_count: usize,
    operator_count: usize,
    contract_count: usize,
    relation_count: usize,
    family_count: usize,
    nodes: Vec<FlywheelNode>,
    contracts: Vec<FlywheelContract>,
    operators: Vec<BlackboxOperator>,
    relations: Vec<BlackboxRelation>,
    families: Vec<BlackboxFamily>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxOperator {
    id: String,
    label: String,
    class: String,
    method: String,
    file: String,
    contract_id: String,
    status: String,
    layer: String,
    called_operators: Vec<String>,
    called_by_nodes: Vec<String>,
    called_by_operators: Vec<String>,
    notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxRelation {
    from: String,
    to: String,
    kind: String,
    depth: usize,
    #[serde(default)]
    via: Vec<String>,
    source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct BlackboxFamily {
    id: String,
    node_count: usize,
    operator_count: usize,
    contract_count: usize,
    nodes: Vec<String>,
    operators: Vec<String>,
    contracts: Vec<String>,
}

#[derive(Debug, Clone)]
struct CatalogNode {
    id: String,
    label: String,
    family: String,
    public_node: bool,
    file: String,
}

#[derive(Debug, Clone)]
struct CatalogOperatorMethod {
    class: String,
    method: String,
    file: String,
}

fn cmd_ledger(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    let operator_filter = cli.flag("operator").map(str::to_ascii_lowercase);
    let node_filter = cli.flag("node").map(str::to_ascii_lowercase);
    let entries: Vec<&LedgerEntry> = ledger
        .entries
        .iter()
        .filter(|entry| {
            operator_filter
                .as_ref()
                .map(|filter| entry.operator.to_ascii_lowercase().contains(filter))
                .unwrap_or(true)
        })
        .filter(|entry| {
            node_filter
                .as_ref()
                .map(|filter| ledger_entry_matches_node(entry, filter))
                .unwrap_or(true)
        })
        .collect();
    let payload = json!({
        "schema_version": ledger.schema_version,
        "entry_count": entries.len(),
        "entries": entries,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_ledger_hygiene(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let files = [LEDGER_PATH, FLYWHEEL_GRAPH_PATH];
    let mut findings = Vec::new();
    for relative in files {
        let path = ctx.devflywheel_dir.join(relative);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        for (index, line) in text.lines().enumerate() {
            let normalized = line.replace('/', "\\").to_ascii_lowercase();
            if normalized.contains("cargo run --manifest-path") {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "direct_cargo_manifest_command",
                    line,
                ));
            }
            if normalized.contains("f:\\cargo-target2\\")
                && normalized.contains("\\debug\\")
                && normalized.contains(".exe")
            {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "direct_target_debug_exe_invocation",
                    line,
                ));
            }
            if normalized.contains("tools\\c3d_devflywheeltool\\run.ps1") {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "legacy_repository_wrapper",
                    line,
                ));
            }
            if normalized.contains("c3d-devflywheeltool ledger") {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "bare_ledger_tool_command",
                    line,
                ));
            }
        }
    }
    let payload = json!({
        "checked_files": files,
        "finding_count": findings.len(),
        "findings": findings,
        "strict": cli.has("strict"),
        "passed": findings.is_empty(),
        "rules": [
            "Ledger and graph records must not contain direct cargo run --manifest-path commands.",
            "Ledger and graph records must not contain direct F:/cargo-target2/.../debug/*.exe invocations.",
            "Ledger and graph records must not contain the retired tools/c3d_devflywheeltool wrapper.",
            "Ledger and graph records must not contain bare c3d-devflywheeltool ledger commands; use Praxis /gaea."
        ],
    });
    print_value(cli.json(), &payload);
    if cli.has("strict") && !payload["passed"].as_bool().unwrap_or(false) {
        return Err(format!(
            "ledger-hygiene found {} violation(s).",
            payload["finding_count"].as_u64().unwrap_or(0)
        ));
    }
    Ok(())
}

fn ledger_hygiene_finding(file: &str, line_number: usize, rule: &str, line: &str) -> Value {
    json!({
        "file": file,
        "line_number": line_number,
        "rule": rule,
        "line": line.trim(),
    })
}

fn cmd_contracts(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    let entries = ledger_entries_for_node(&ledger, &node);
    let payload = json!({
        "schema_version": ledger.schema_version,
        "node": node,
        "entry_count": entries.len(),
        "status_counts": ledger_status_counts(&entries),
        "layer_summaries": ledger_layer_summaries(&entries),
        "entries": entries,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_status(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let payload = status_payload(ctx, &node)?;
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_goal_chain_status(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let nodes = cli
        .flag("nodes")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|node| !node.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|nodes| !nodes.is_empty())
        .unwrap_or_else(|| {
            [
                "ThermalShaper",
                "Weathering",
                "Snowfield",
                "Glacier",
                "Debris",
            ]
            .iter()
            .map(|node| (*node).to_string())
            .collect()
        });
    let statuses = nodes
        .iter()
        .map(|node| status_payload(ctx, node))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = statuses
        .iter()
        .map(goal_chain_status_row)
        .collect::<Vec<_>>();
    let node_count = rows.len() as u64;
    let contract_gate_count = rows
        .iter()
        .filter(|row| row.pointer("/gates/contract").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let exact_gate_count = rows
        .iter()
        .filter(|row| row.pointer("/gates/exact").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let accepted_gate_count = rows
        .iter()
        .filter(|row| row.pointer("/gates/accepted").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let open_contract_count = rows
        .iter()
        .filter_map(|row| row.get("open_contract_count").and_then(Value::as_u64))
        .sum::<u64>();
    let conflict_count = rows
        .iter()
        .filter(|row| row.get("ledger_artifact_conflict").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let weakest_nodes = rows
        .iter()
        .filter(|row| {
            row.pointer("/gates/contract").and_then(Value::as_bool) != Some(true)
                || row
                    .get("open_contract_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
        })
        .filter_map(|row| row.get("node").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let next_focus = if weakest_nodes.contains(&"Weathering") {
        "Weathering full-node AO/color branches remain open; height-chain base scalar scope is accounted."
    } else if contract_gate_count < node_count {
        "Promote or rerun the nodes without a contract gate before widening performance claims."
    } else {
        "All requested nodes have a scoped contract gate; next useful work is chain-level resident CPU/GPU scheduling and wider performance acceptance."
    };
    let payload = json!({
        "command": "goal-chain-status",
        "chain": nodes,
        "summary": {
            "node_count": node_count,
            "contract_gate_count": contract_gate_count,
            "exact_gate_count": exact_gate_count,
            "accepted_gate_count": accepted_gate_count,
            "open_contract_count": open_contract_count,
            "ledger_artifact_conflict_count": conflict_count,
            "all_contract_gated": contract_gate_count == node_count,
            "all_exact_or_accepted": contract_gate_count == node_count,
            "all_bit_exact": exact_gate_count == node_count,
            "weakest_nodes": weakest_nodes,
            "next_focus": next_focus,
        },
        "nodes": rows,
        "truth_rule": "This is a flywheel status rollup only; it does not execute fresh probes or claim full-node closure beyond each node's promotion_scope.",
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_goal_chain_bench(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let run_dir = ctx
        .artifact_root
        .join("goal-chain-bench")
        .join(unix_stamp_millis().to_string());
    let command = goal_chain_bench_command(ctx, cli, &run_dir);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "goal-chain-bench",
            "artifact_dir": path_text(&run_dir),
            "probe_command": command_preview(&command),
            "truth_rule": "Executes a fresh native ThermalShaper -> Weathering -> Snowfield -> Glacier -> Debris buffer/runtime bench; it is not a Gaea Bridge parity claim."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let output = run_capture(command)?;
    let stdout_json = extract_jsonish(&output.stdout).unwrap_or_else(|| output.stdout.clone());
    let mut report: Value = serde_json::from_str(&stdout_json).map_err(|error| {
        format!("Failed to parse goal-chain bench JSON: {error}\n{stdout_json}")
    })?;
    report["artifact_dir"] = json!(path_text(&run_dir));
    report["probe_stderr"] = json!(output.stderr);
    report["tool_command"] = json!("goal-chain-bench");
    let report_path = run_dir.join("goal_chain_bench_report.json");
    report["artifact_report_path"] = json!(path_text(&report_path));
    write_pretty_json(&report_path, &report)?;
    print_value(cli.json(), &report);
    if cli.has("require-all-pass") && report.get("all_pass").and_then(Value::as_bool) != Some(true)
    {
        return Err("goal-chain-bench failed --require-all-pass.".to_string());
    }
    Ok(())
}

fn goal_chain_bench_command(ctx: &Context, cli: &Cli, run_dir: &Path) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_goal_chain_native_bench");
    command.arg("--dump-dir").arg(run_dir);
    command.arg("--json");
    for key in [
        "resolution",
        "terrain-width",
        "terrain-height",
        "source",
        "repeat",
        "target-total-ms",
        "thermal-backend",
        "thermal-scale",
        "thermal-influence",
        "thermal-shape",
        "thermal-microdetail-preservation",
        "weathering-scale",
        "weathering-creep",
        "weathering-amount",
        "weathering-dirt",
        "weathering-backend",
        "snowfield-backend",
        "snowfield-cascades",
        "snowfield-duration",
        "snowfield-intensity",
        "snowfield-melt",
        "glacier-backend",
        "glacier-breakage-count",
        "debris-amount",
        "debris-seed",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}")).arg(value);
        }
    }
    for key in [
        "require-all-pass",
        "require-consistent",
        "require-finite",
        "require-performance",
        "glacier-rough-edges",
        "glacier-diagonal-breakage",
        "glacier-flow-breakage",
        "debris-point-cloud",
        "debris-export-point-cloud",
    ] {
        if cli.has(key) {
            command.arg(format!("--{key}"));
        }
    }
    append_passthrough_args(&mut command, cli);
    command
}

fn goal_chain_status_row(status: &Value) -> Value {
    json!({
        "node": status.get("node"),
        "state": status.get("state"),
        "readiness": status.pointer("/promotion_readiness/readiness"),
        "score_percent": status.pointer("/headline/contract_score_percent"),
        "latest_audit_exact_percent": status.pointer("/headline/latest_audit_exact_percent"),
        "latest_audit_accepted_percent": status.pointer("/headline/latest_audit_accepted_percent"),
        "gates": {
            "exact": status.pointer("/headline/artifact_exact_gate"),
            "accepted": status.pointer("/headline/artifact_acceptance_gate"),
            "contract": status.pointer("/headline/artifact_contract_gate"),
        },
        "promotion_scope": status.pointer("/artifact_scope/promotion_scope"),
        "matched_contracts": status
            .pointer("/artifact_scope/matched_contracts")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "open_contract_count": status.pointer("/headline/open_contract_count"),
        "ledger_artifact_conflict": status.pointer("/headline/ledger_artifact_conflict"),
        "latest_audit_artifact": status.pointer("/artifacts/latest_audit_artifact"),
    })
}

fn cmd_open_frontier(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let ledger = read_ledger(ctx)?;
    let node_filter = cli.flag("node").map(str::to_ascii_lowercase);
    let include_all_unclosed = cli.has("all");
    let mut selected_by_node: BTreeMap<String, Vec<&LedgerEntry>> = BTreeMap::new();
    for entry in &ledger.entries {
        if let Some(filter) = &node_filter {
            if entry.node.to_ascii_lowercase() != *filter {
                continue;
            }
        }
        let selected = entry.status == "open"
            || (include_all_unclosed && !is_audited_contract_status(&entry.status));
        if selected {
            selected_by_node
                .entry(entry.node.clone())
                .or_default()
                .push(entry);
        }
    }

    let mut nodes = selected_by_node
        .into_iter()
        .map(|(node, selected_entries)| {
            let all_entries = ledger_entries_for_node(&ledger, &node);
            let open_entries = all_entries
                .iter()
                .filter(|entry| entry.status == "open")
                .copied()
                .collect::<Vec<_>>();
            let mut blocking_layers = BTreeSet::new();
            for entry in &selected_entries {
                blocking_layers.insert(entry.layer.clone());
            }
            json!({
                "node": &node,
                "selected_entry_count": selected_entries.len(),
                "open_entry_count": open_entries.len(),
                "contract_score_percent": round1(ledger_contract_score(&all_entries)),
                "status_counts": ledger_status_counts(&all_entries),
                "blocking_layers": blocking_layers.into_iter().collect::<Vec<_>>(),
                "entries": selected_entries
                    .iter()
                    .map(|entry| {
                        json!({
                            "operator": &entry.operator,
                            "layer": &entry.layer,
                            "status": &entry.status,
                            "latest_native_evidence": entry.native_evidence.last(),
                            "latest_rust_implementation": entry.rust_implementation.last(),
                            "evidence_summary": &entry.evidence_summary,
                            "open_risk": &entry.open_risk,
                        })
                    })
                    .collect::<Vec<_>>(),
                "recommended_next_commands": open_frontier_recommendations(&node),
            })
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|a, b| {
        let a_score = a
            .get("contract_score_percent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let b_score = b
            .get("contract_score_percent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        a_score
            .partial_cmp(&b_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.get("open_entry_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .cmp(
                        &a.get("open_entry_count")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
            })
    });

    let payload = json!({
        "schema_version": ledger.schema_version,
        "command": "open-frontier",
        "mode": if include_all_unclosed { "all_unclosed" } else { "open_only" },
        "node_filter": node_filter,
        "node_count": nodes.len(),
        "nodes": nodes,
        "truth_rule": "Open frontier is a ledger triage view. It does not promote parity; raw Bridge-vs-Native evidence and ledger status must still agree."
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_acceptance_matrix(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let path = ctx
        .devflywheel_dir
        .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH);
    let mut payload: Value = read_json(&path)?;
    if let Some(node_filter) = cli.flag("node") {
        if let Some(rows) = payload.get("rows").and_then(Value::as_array) {
            let filtered_rows = rows
                .iter()
                .filter(|row| {
                    row.get("node")
                        .and_then(Value::as_str)
                        .map(|node| node.eq_ignore_ascii_case(node_filter))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(object) = payload.as_object_mut() {
                object.insert("node_filter".to_string(), json!(node_filter));
                object.insert("row_count".to_string(), json!(filtered_rows.len()));
                object.insert("rows".to_string(), json!(filtered_rows));
            }
        }
    }
    if let Some(object) = payload.as_object_mut() {
        object.insert("path".to_string(), json!(path));
        object.insert(
            "truth_rule".to_string(),
            json!("Speed claims require exact raw Bridge/native closure plus an explicit baseline source. Gaea desktop app baselines are preferred; direct Gaea harness method timings are acceptable only when the row declares baseline_source=gaea_bridge_harness_method_elapsed."),
        );
    }
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_graph(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let payload = flywheel_graph_payload(ctx)?;
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_impact(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let target = cli
        .flag("operator")
        .or_else(|| cli.flag("contract"))
        .or_else(|| cli.flag("substrate"))
        .unwrap_or("pe");
    let graph = read_flywheel_graph(ctx)?;
    let ledger = read_ledger(ctx)?;
    let mut matches = Vec::new();
    for contract in &graph.contracts {
        if contract_matches(contract, target) {
            matches.push(flywheel_contract_view(contract, &ledger));
        }
    }
    let mut unlocked_nodes = BTreeSet::new();
    for contract in &graph.contracts {
        if contract_matches(contract, target) {
            for node in &contract.unlocks {
                unlocked_nodes.insert(node.clone());
            }
        }
    }
    let affected_nodes = unlocked_nodes
        .iter()
        .filter_map(|node| {
            graph
                .nodes
                .iter()
                .find(|candidate| candidate.id.eq_ignore_ascii_case(node))
        })
        .map(|node| flywheel_node_plan_view(node, &graph, &ledger))
        .collect::<Vec<_>>();
    let payload = json!({
        "schema_version": graph.schema_version,
        "query": target,
        "matched_contract_count": matches.len(),
        "matched_contracts": matches,
        "affected_node_count": affected_nodes.len(),
        "affected_nodes": affected_nodes,
        "truth_rule": "Impact is computed from the flywheel graph plus ledger statuses; closed substrate contracts unlock downstream nodes but do not replace raw parity proof."
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_plan(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let graph = read_flywheel_graph(ctx)?;
    let ledger = read_ledger(ctx)?;
    let Some(target) = graph
        .nodes
        .iter()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&node))
    else {
        return Err(format!(
            "Unknown flywheel node '{node}'. Run '{TOOL_COMMAND} graph --json'."
        ));
    };
    let payload = flywheel_node_plan_view(target, &graph, &ledger);
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_export_ui(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let payload = flywheel_ui_payload(ctx)?;
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_blackbox_scan(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let inventory = build_blackbox_inventory(ctx)?;
    let path = ctx.devflywheel_dir.join(BLACKBOX_INVENTORY_PATH);
    if !cli.has("dry-run") {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
        }
        write_pretty_json(&path, &json!(inventory))?;
    }
    let payload = json!({
        "schema_version": inventory.schema_version,
        "generated_by": inventory.generated_by,
        "path": path,
        "written": !cli.has("dry-run"),
        "public_or_operator_node_count": inventory.node_count,
        "operator_count": inventory.operator_count,
        "contract_count": inventory.contract_count,
        "relation_count": inventory.relation_count,
        "family_count": inventory.family_count,
        "open_contract_samples": inventory.contracts.iter().take(24).map(|contract| contract.id.clone()).collect::<Vec<_>>(),
        "truth_rule": "This inventory is static reverse evidence. It maps blackbox/common dependencies and best-effort port shells; raw buffer parity plus decompiled parameter/port surface parity are both required before any node is marked closed."
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn read_ledger(ctx: &Context) -> Result<Ledger, String> {
    read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))
}

fn read_flywheel_graph(ctx: &Context) -> Result<FlywheelGraph, String> {
    let mut graph: FlywheelGraph = read_json(&ctx.devflywheel_dir.join(FLYWHEEL_GRAPH_PATH))?;
    merge_blackbox_inventory(ctx, &mut graph)?;
    Ok(graph)
}

fn read_base_flywheel_graph(ctx: &Context) -> Result<FlywheelGraph, String> {
    read_json(&ctx.devflywheel_dir.join(FLYWHEEL_GRAPH_PATH))
}

fn merge_blackbox_inventory(ctx: &Context, graph: &mut FlywheelGraph) -> Result<(), String> {
    let path = ctx.devflywheel_dir.join(BLACKBOX_INVENTORY_PATH);
    if !path.exists() {
        return Ok(());
    }
    let inventory: BlackboxInventory = read_json(&path)?;
    for contract in inventory.contracts {
        merge_contract_into_graph(graph, contract);
    }
    for node in inventory.nodes {
        merge_node_into_graph(graph, node);
    }
    Ok(())
}

fn merge_contract_into_graph(graph: &mut FlywheelGraph, contract: FlywheelContract) {
    if let Some(existing) = graph
        .contracts
        .iter_mut()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&contract.id))
    {
        merge_strings(&mut existing.ledger_operators, &contract.ledger_operators);
        merge_strings(&mut existing.owner_nodes, &contract.owner_nodes);
        merge_strings(&mut existing.unlocks, &contract.unlocks);
        merge_strings(&mut existing.implementation, &contract.implementation);
        merge_strings(&mut existing.evidence, &contract.evidence);
        merge_strings(&mut existing.next_commands, &contract.next_commands);
        if existing.status.is_none() {
            existing.status = contract.status;
        }
        if existing.notes.is_empty() {
            existing.notes = contract.notes;
        }
        return;
    }
    graph.contracts.push(contract);
}

fn merge_node_into_graph(graph: &mut FlywheelGraph, node: FlywheelNode) {
    if let Some(existing) = graph
        .nodes
        .iter_mut()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&node.id))
    {
        merge_strings(&mut existing.depends_on, &node.depends_on);
        merge_strings(&mut existing.outputs, &node.outputs);
        merge_strings(&mut existing.shared_operators, &node.shared_operators);
        merge_strings(&mut existing.recipe_families, &node.recipe_families);
        merge_strings(&mut existing.next_commands, &node.next_commands);
        merge_ports(&mut existing.input_ports, &node.input_ports);
        merge_ports(&mut existing.output_ports, &node.output_ports);
        if existing.notes.is_empty() {
            existing.notes = node.notes;
        }
        return;
    }
    graph.nodes.push(node);
}

fn merge_strings(target: &mut Vec<String>, incoming: &[String]) {
    target.extend(incoming.iter().cloned());
    dedup_strings(target);
}

fn merge_ports(target: &mut Vec<FlywheelPort>, incoming: &[FlywheelPort]) {
    for port in incoming {
        if let Some(existing) = target
            .iter_mut()
            .find(|candidate| same_port(candidate, port))
        {
            if existing.required.is_none() {
                existing.required = port.required;
            }
            if existing.slot.is_none() {
                existing.slot = port.slot;
            }
            if existing.source_slot.is_none() {
                existing.source_slot = port.source_slot;
            }
        } else {
            target.push(port.clone());
        }
    }
}

fn same_port(lhs: &FlywheelPort, rhs: &FlywheelPort) -> bool {
    lhs.name.eq_ignore_ascii_case(&rhs.name)
        && lhs.role.eq_ignore_ascii_case(&rhs.role)
        && (lhs.slot == rhs.slot || lhs.slot.is_none() || rhs.slot.is_none())
        && (lhs.source_slot == rhs.source_slot
            || lhs.source_slot.is_none()
            || rhs.source_slot.is_none())
}

fn build_blackbox_inventory(ctx: &Context) -> Result<BlackboxInventory, String> {
    let base_graph = read_base_flywheel_graph(ctx)?;
    let catalog_nodes = read_catalog_nodes(ctx)?;
    let mut operator_methods = read_catalog_operator_methods(ctx)?;
    operator_methods.extend(scan_core_operator_methods(ctx)?);
    dedup_operator_methods(&mut operator_methods);

    let class_set = blackbox_class_set(&operator_methods);
    let existing_contracts = base_graph
        .contracts
        .iter()
        .map(|contract| contract.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut generated_contracts = BTreeMap::<String, FlywheelContract>::new();
    let mut nodes = Vec::<FlywheelNode>::new();
    let mut relations = Vec::<BlackboxRelation>::new();
    let mut called_by_nodes = BTreeMap::<String, Vec<String>>::new();
    let mut operator_calls = BTreeMap::<String, Vec<(String, String)>>::new();

    for method in &operator_methods {
        let path = resolve_operator_source_path(ctx, method);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let body = extract_method_body(&text, &method.method).unwrap_or(text);
        let calls = extract_blackbox_calls(&body, &class_set)
            .into_iter()
            .filter(|(class, called)| {
                !(class.eq_ignore_ascii_case(&method.class)
                    && called.eq_ignore_ascii_case(&method.method))
            })
            .collect::<Vec<_>>();
        for (class, called) in &calls {
            if mapped_contract_id(class, called).is_none() {
                let operator = CatalogOperatorMethod {
                    class: class.clone(),
                    method: called.clone(),
                    file: source_file_for_class(ctx, class)
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                };
                ensure_operator_contract(
                    &operator,
                    operator.file.clone(),
                    &existing_contracts,
                    &mut generated_contracts,
                );
            }
        }
        operator_calls.insert(operator_key(&method.class, &method.method), calls);
        ensure_operator_contract(
            method,
            path.display().to_string(),
            &existing_contracts,
            &mut generated_contracts,
        );
    }

    for catalog_node in catalog_nodes.iter().filter(|node| node.public_node) {
        let path = resolve_node_source_path(ctx, &catalog_node.file);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let calls = extract_blackbox_calls(&text, &class_set);
        let mut depends_on = Vec::new();
        let mut shared_operators = Vec::new();
        for (class, method) in &calls {
            let contract_id = contract_id_for_call(class, method);
            push_unique_string(&mut depends_on, &contract_id);
            push_unique_string(&mut shared_operators, &format!("{class}.{method}"));
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: format!("op:{}", operator_key(class, method)),
                kind: "node_calls_operator".to_string(),
                depth: 0,
                via: Vec::new(),
                source: path.display().to_string(),
            });
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: format!("contract:{contract_id}"),
                kind: "node_direct_depends_on_contract".to_string(),
                depth: 0,
                via: vec![format!("op:{}", operator_key(class, method))],
                source: path.display().to_string(),
            });
            called_by_nodes
                .entry(operator_key(class, method))
                .or_default()
                .push(catalog_node.id.clone());
            if mapped_contract_id(class, method).is_none() {
                let operator = CatalogOperatorMethod {
                    class: class.clone(),
                    method: method.clone(),
                    file: source_file_for_class(ctx, class)
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                };
                ensure_operator_contract(
                    &operator,
                    operator.file.clone(),
                    &existing_contracts,
                    &mut generated_contracts,
                );
            }
        }
        for dependency in collect_transitive_contract_dependencies(&calls, &operator_calls, 8) {
            push_unique_string(&mut depends_on, &dependency.contract_id);
            push_unique_string(&mut shared_operators, &dependency.operator);
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: format!("contract:{}", dependency.contract_id),
                kind: "node_transitive_depends_on_contract".to_string(),
                depth: dependency.depth,
                via: dependency.via,
                source: path.display().to_string(),
            });
        }
        if text.contains("Commit(") || text.contains("Map ") || text.contains("Map[]") {
            push_unique_string(&mut depends_on, "heightfield.map.scalar_first_ports");
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: "contract:heightfield.map.scalar_first_ports".to_string(),
                kind: "node_uses_heightfield_map_contract".to_string(),
                depth: 0,
                via: Vec::new(),
                source: path.display().to_string(),
            });
        }
        let (input_ports, output_ports) = extract_node_ports(&text, catalog_node);
        let outputs = output_ports
            .iter()
            .map(|port| port.name.clone())
            .collect::<Vec<_>>();
        nodes.push(FlywheelNode {
            id: catalog_node.id.clone(),
            label: catalog_node.label.clone(),
            domain: format!("Gaea {} heightfield", catalog_node.family),
            kind: classify_public_node_kind(&text, &input_ports, &output_ports).to_string(),
            priority: candidate_priority(&catalog_node.id).to_string(),
            depends_on,
            outputs,
            input_ports,
            output_ports,
            shared_operators,
            recipe_families: vec![catalog_node.family.clone()],
            next_commands: vec![
                format!("{TOOL_COMMAND} reverse --node {} --json", catalog_node.id),
                format!("{TOOL_COMMAND} plan --node {} --json", catalog_node.id),
            ],
            notes: node_inventory_notes(catalog_node),
        });
    }

    let mut called_by_operators = BTreeMap::<String, Vec<String>>::new();
    for (owner, calls) in &operator_calls {
        for (class, method) in calls {
            let contract_id = contract_id_for_call(class, method);
            relations.push(BlackboxRelation {
                from: format!("op:{owner}"),
                to: format!("op:{}", operator_key(class, method)),
                kind: "operator_calls_operator".to_string(),
                depth: 0,
                via: Vec::new(),
                source: String::new(),
            });
            relations.push(BlackboxRelation {
                from: format!("op:{owner}"),
                to: format!("contract:{contract_id}"),
                kind: "operator_depends_on_contract".to_string(),
                depth: 0,
                via: vec![format!("op:{}", operator_key(class, method))],
                source: String::new(),
            });
            called_by_operators
                .entry(operator_key(class, method))
                .or_default()
                .push(owner.clone());
        }
    }
    let called_by_nodes_snapshot = called_by_nodes.clone();
    for method in &operator_methods {
        let contract_id = contract_id_for_call(&method.class, &method.method);
        if let Some(contract) = generated_contracts.get_mut(&contract_id) {
            let key = operator_key(&method.class, &method.method);
            if let Some(nodes) = called_by_nodes_snapshot.get(&key) {
                merge_strings(&mut contract.unlocks, nodes);
                for node in nodes {
                    relations.push(BlackboxRelation {
                        from: format!("contract:{contract_id}"),
                        to: format!("node:{node}"),
                        kind: "contract_unlocks_node".to_string(),
                        depth: 0,
                        via: vec![format!("op:{key}")],
                        source: String::new(),
                    });
                }
            }
        }
    }

    for method in &operator_methods {
        let key = operator_key(&method.class, &method.method);
        let calls = operator_calls.get(&key).cloned().unwrap_or_default();
        let mut depends_on = Vec::new();
        let mut shared_operators = Vec::new();
        for (class, called_method) in &calls {
            let contract_id = contract_id_for_call(class, called_method);
            if !contract_id
                .eq_ignore_ascii_case(&contract_id_for_call(&method.class, &method.method))
            {
                push_unique_string(&mut depends_on, &contract_id);
            }
            push_unique_string(&mut shared_operators, &format!("{class}.{called_method}"));
        }
        for dependency in collect_transitive_contract_dependencies(&calls, &operator_calls, 8) {
            if !dependency
                .contract_id
                .eq_ignore_ascii_case(&contract_id_for_call(&method.class, &method.method))
            {
                push_unique_string(&mut depends_on, &dependency.contract_id);
            }
            push_unique_string(&mut shared_operators, &dependency.operator);
        }
        nodes.push(FlywheelNode {
            id: format!("op.{}.{}", method.class, method.method),
            label: format!("{}.{}", method.class, method.method),
            domain: "Gaea shared blackbox function".to_string(),
            kind: "blackbox_function".to_string(),
            priority: if called_by_nodes.get(&key).map(Vec::len).unwrap_or(0) > 0 {
                "medium"
            } else {
                "low"
            }
            .to_string(),
            depends_on,
            outputs: Vec::new(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            shared_operators,
            recipe_families: vec![operator_family_for_class(&method.class).to_string()],
            next_commands: vec![format!("{TOOL_COMMAND} impact --operator {} --json", method.class)],
            notes: "Operator-level blackbox node; closing it should migrate reusable substrate before node recipe glue.".to_string(),
        });
    }

    let mut operators = Vec::new();
    for method in &operator_methods {
        let key = operator_key(&method.class, &method.method);
        operators.push(BlackboxOperator {
            id: key.clone(),
            label: format!("{}.{}", method.class, method.method),
            class: method.class.clone(),
            method: method.method.clone(),
            file: method.file.clone(),
            contract_id: contract_id_for_call(&method.class, &method.method),
            status: if mapped_contract_id(&method.class, &method.method).is_some() {
                "mapped_existing"
            } else {
                "open"
            }
            .to_string(),
            layer: layer_for_class(&method.class).to_string(),
            called_operators: sorted_strings(
                operator_calls
                    .get(&key)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(class, method)| operator_key(&class, &method))
                    .collect(),
            ),
            called_by_nodes: sorted_strings(called_by_nodes.remove(&key).unwrap_or_default()),
            called_by_operators: sorted_strings(
                called_by_operators.remove(&key).unwrap_or_default(),
            ),
            notes: "Static entry or core helper from decompiled Gaea source.".to_string(),
        });
    }

    let mut contracts = generated_contracts.into_values().collect::<Vec<_>>();
    contracts.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    nodes.sort_by(|lhs, rhs| {
        priority_rank_text(&lhs.priority)
            .cmp(&priority_rank_text(&rhs.priority))
            .then_with(|| lhs.kind.cmp(&rhs.kind))
            .then_with(|| lhs.id.cmp(&rhs.id))
    });
    operators.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    dedup_relations(&mut relations);
    let families = build_blackbox_families(&nodes, &operators);
    relations.extend(family_relations(&families));
    dedup_relations(&mut relations);
    relations.sort_by(|lhs, rhs| {
        lhs.from
            .cmp(&rhs.from)
            .then_with(|| lhs.kind.cmp(&rhs.kind))
            .then_with(|| lhs.to.cmp(&rhs.to))
            .then_with(|| lhs.depth.cmp(&rhs.depth))
    });

    Ok(BlackboxInventory {
        schema_version: 1,
        generated_by: format!("{TOOL_COMMAND} blackbox-scan"),
        generated_from: gaea_nodes_source_dir(ctx).display().to_string(),
        node_count: nodes.len(),
        operator_count: operators.len(),
        contract_count: contracts.len(),
        relation_count: relations.len(),
        family_count: families.len(),
        nodes,
        contracts,
        operators,
        relations,
        families,
    })
}

fn ensure_operator_contract(
    method: &CatalogOperatorMethod,
    file: String,
    existing_contracts: &BTreeSet<String>,
    generated_contracts: &mut BTreeMap<String, FlywheelContract>,
) {
    if mapped_contract_id(&method.class, &method.method).is_some() {
        return;
    }
    let id = contract_id_for_call(&method.class, &method.method);
    if existing_contracts.contains(&id.to_ascii_lowercase())
        || generated_contracts.contains_key(&id)
    {
        return;
    }
    generated_contracts.insert(
        id.clone(),
        FlywheelContract {
            id,
            label: format!("{}.{}", method.class, method.method),
            kind: "auto_blackbox_operator".to_string(),
            layer: layer_for_class(&method.class).to_string(),
            status: Some("open".to_string()),
            ledger_operators: Vec::new(),
            owner_nodes: vec![format!("op.{}.{}", method.class, method.method)],
            reusable: true,
            unlocks: Vec::new(),
            implementation: if file.is_empty() { Vec::new() } else { vec![file] },
            evidence: vec!["D:\\ghost1.0\\_gaea_decompiled\\_summary\\gaea_nodes_and_operators_catalog.md".to_string()],
            next_commands: vec![format!("{TOOL_COMMAND} impact --operator {} --json", method.class)],
            notes: "Auto-scanned blackbox function. Promote only after clean-room substrate migration and raw parity evidence.".to_string(),
        },
    );
}

#[derive(Debug, Clone)]
struct TransitiveDependency {
    contract_id: String,
    operator: String,
    depth: usize,
    via: Vec<String>,
}

fn collect_transitive_contract_dependencies(
    roots: &[(String, String)],
    operator_calls: &BTreeMap<String, Vec<(String, String)>>,
    max_depth: usize,
) -> Vec<TransitiveDependency> {
    let mut out = Vec::new();
    let mut seen_edges = BTreeSet::new();
    let mut stack = roots
        .iter()
        .map(|(class, method)| {
            (
                operator_key(class, method),
                1usize,
                vec![format!("op:{}", operator_key(class, method))],
            )
        })
        .collect::<Vec<_>>();
    while let Some((operator, depth, via)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let Some(calls) = operator_calls.get(&operator) else {
            continue;
        };
        for (class, method) in calls {
            let called_operator = operator_key(class, method);
            let edge_key = format!("{operator}->{called_operator}:{depth}");
            if !seen_edges.insert(edge_key) {
                continue;
            }
            let mut next_via = via.clone();
            next_via.push(format!("op:{called_operator}"));
            out.push(TransitiveDependency {
                contract_id: contract_id_for_call(class, method),
                operator: format!("{class}.{method}"),
                depth,
                via: next_via.clone(),
            });
            if depth < max_depth {
                stack.push((called_operator, depth + 1, next_via));
            }
        }
    }
    out.sort_by(|lhs, rhs| {
        lhs.contract_id
            .cmp(&rhs.contract_id)
            .then_with(|| lhs.depth.cmp(&rhs.depth))
    });
    let mut seen_contracts = BTreeSet::new();
    out.retain(|dependency| seen_contracts.insert(dependency.contract_id.to_ascii_lowercase()));
    out
}

fn dedup_relations(relations: &mut Vec<BlackboxRelation>) {
    let mut seen = BTreeSet::new();
    relations.retain(|relation| {
        seen.insert(format!(
            "{}|{}|{}|{}|{}",
            relation.from,
            relation.to,
            relation.kind,
            relation.depth,
            relation.via.join(">")
        ))
    });
}

fn build_blackbox_families(
    nodes: &[FlywheelNode],
    operators: &[BlackboxOperator],
) -> Vec<BlackboxFamily> {
    let mut map = BTreeMap::<String, BlackboxFamily>::new();
    for node in nodes {
        for family in &node.recipe_families {
            let entry = map.entry(family.clone()).or_insert_with(|| BlackboxFamily {
                id: family.clone(),
                node_count: 0,
                operator_count: 0,
                contract_count: 0,
                nodes: Vec::new(),
                operators: Vec::new(),
                contracts: Vec::new(),
            });
            push_unique_string(&mut entry.nodes, &node.id);
            for dependency in &node.depends_on {
                push_unique_string(&mut entry.contracts, dependency);
            }
        }
    }
    for operator in operators {
        let family = operator_family_for_class(&operator.class).to_string();
        let entry = map.entry(family.clone()).or_insert_with(|| BlackboxFamily {
            id: family,
            node_count: 0,
            operator_count: 0,
            contract_count: 0,
            nodes: Vec::new(),
            operators: Vec::new(),
            contracts: Vec::new(),
        });
        push_unique_string(&mut entry.operators, &operator.id);
        push_unique_string(&mut entry.contracts, &operator.contract_id);
    }
    let mut families = map.into_values().collect::<Vec<_>>();
    for family in &mut families {
        family.nodes = sorted_strings(std::mem::take(&mut family.nodes));
        family.operators = sorted_strings(std::mem::take(&mut family.operators));
        family.contracts = sorted_strings(std::mem::take(&mut family.contracts));
        family.node_count = family.nodes.len();
        family.operator_count = family.operators.len();
        family.contract_count = family.contracts.len();
    }
    families.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    families
}

fn family_relations(families: &[BlackboxFamily]) -> Vec<BlackboxRelation> {
    let mut relations = Vec::new();
    for family in families {
        for node in &family.nodes {
            relations.push(BlackboxRelation {
                from: format!("family:{}", family.id),
                to: format!("node:{node}"),
                kind: "family_contains_node".to_string(),
                depth: 0,
                via: Vec::new(),
                source: "blackbox_family_aggregate".to_string(),
            });
        }
        for operator in &family.operators {
            relations.push(BlackboxRelation {
                from: format!("family:{}", family.id),
                to: format!("op:{operator}"),
                kind: "family_contains_operator".to_string(),
                depth: 0,
                via: Vec::new(),
                source: "blackbox_family_aggregate".to_string(),
            });
        }
        for contract in &family.contracts {
            relations.push(BlackboxRelation {
                from: format!("family:{}", family.id),
                to: format!("contract:{contract}"),
                kind: "family_depends_on_contract".to_string(),
                depth: 0,
                via: Vec::new(),
                source: "blackbox_family_aggregate".to_string(),
            });
        }
    }
    relations
}

fn read_catalog_nodes(ctx: &Context) -> Result<Vec<CatalogNode>, String> {
    let path = ctx.summary_dir.join("gaea_nodes_and_operators_catalog.md");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let mut family = String::new();
    let mut nodes = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("### ") {
            family = name.split('(').next().unwrap_or(name).trim().to_string();
            continue;
        }
        let Some(rest) = line.strip_prefix("- `") else {
            continue;
        };
        let parts = line.split('`').collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }
        let id = rest.split('`').next().unwrap_or_default().trim();
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            continue;
        }
        let label = parts.get(3).copied().unwrap_or(id).trim();
        let file = parts
            .iter()
            .rev()
            .find(|part| part.ends_with(".cs"))
            .copied()
            .unwrap_or_default()
            .to_string();
        if file.is_empty() {
            continue;
        }
        nodes.push(CatalogNode {
            id: id.to_string(),
            label: if label.is_empty() {
                id.to_string()
            } else {
                label.to_string()
            },
            family: family.clone(),
            public_node: line.contains("| public |"),
            file,
        });
    }
    Ok(nodes)
}

fn read_catalog_operator_methods(ctx: &Context) -> Result<Vec<CatalogOperatorMethod>, String> {
    let path = ctx.summary_dir.join("gaea_nodes_and_operators_catalog.md");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let mut in_operator_section = false;
    let mut class = String::new();
    let mut file = String::new();
    let mut methods = Vec::new();
    for line in text.lines() {
        if line == "## Algorithm Operator Entry Classes" {
            in_operator_section = true;
            continue;
        }
        if in_operator_section
            && line.starts_with("## ")
            && line != "## Algorithm Operator Entry Classes"
        {
            break;
        }
        if !in_operator_section {
            continue;
        }
        if let Some(rest) = line.strip_prefix("### `") {
            class = rest.split('`').next().unwrap_or_default().to_string();
            file.clear();
            continue;
        }
        if line.starts_with("- File: `") {
            file = line.split('`').nth(1).unwrap_or_default().to_string();
            continue;
        }
        if line.starts_with("- Static methods") {
            for method in coded_segments(line)
                .into_iter()
                .filter(|method| method != &class)
            {
                methods.push(CatalogOperatorMethod {
                    class: class.clone(),
                    method,
                    file: file.clone(),
                });
            }
        }
    }
    Ok(methods)
}

fn scan_core_operator_methods(ctx: &Context) -> Result<Vec<CatalogOperatorMethod>, String> {
    let roots = [
        gaea_nodes_source_dir(ctx).join("Core"),
        gaea_engine_source_dir(ctx).join("Processing"),
        gaea_engine_source_dir(ctx).join("Utilities"),
    ];
    let mut methods = Vec::new();
    for root in roots {
        for path in collect_cs_files_checked(&root)? {
            let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            if is_decompiler_generated_class(stem) || !is_shared_blackbox_source(stem) {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap_or_default();
            let class = primary_source_type_name(&text).unwrap_or_else(|| stem.to_string());
            if is_decompiler_generated_class(&class) || !is_shared_blackbox_source(&class) {
                continue;
            }
            if path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("Utilities")
            }) && !is_allowed_engine_utility_class(&class)
            {
                continue;
            }
            for method in extract_static_method_names(&text) {
                methods.push(CatalogOperatorMethod {
                    class: class.clone(),
                    method,
                    file: path.display().to_string(),
                });
            }
        }
    }
    Ok(methods)
}
