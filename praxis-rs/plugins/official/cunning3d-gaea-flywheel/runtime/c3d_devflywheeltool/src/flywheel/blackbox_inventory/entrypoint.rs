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
        "houdini-fuse-capture" | "houdini-fuse-oracle" => cmd_houdini_fuse_capture(&ctx, &cli),
        "houdini-fuse-compare" | "houdini-fuse-parity" => cmd_houdini_fuse_compare(&ctx, &cli),
        "houdini-fuse-benchmark" | "houdini-fuse-performance" => {
            cmd_houdini_fuse_benchmark(&ctx, &cli)
        }
        "houdini-group-capture" | "houdini-group-oracle" => cmd_houdini_group_capture(&ctx, &cli),
        "houdini-group-native-capture" => cmd_houdini_group_native_capture(&ctx, &cli),
        "houdini-group-native-performance" => cmd_houdini_group_native_performance(&ctx, &cli),
        "houdini-group-native-path-profile" => cmd_houdini_group_native_path_profile(&ctx, &cli),
        "houdini-group-compare" | "houdini-group-parity" => cmd_houdini_group_compare(&ctx, &cli),
        "houdini-group-performance-compare" | "houdini-group-benchmark-compare" => {
            cmd_houdini_group_performance_compare(&ctx, &cli)
        }
        "houdini-native-reverse" | "houdini-reverse" => cmd_houdini_native_reverse(&ctx, &cli),
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
        "harness-build" | "build-harness" => cmd_harness_build(&ctx, &cli),
        "architecture-guard" | "cce-architecture-guard" => {
            architecture_guard::cmd_architecture_guard(&ctx, &cli)
        }
        "cce-graph-run" | "canonical-cce-graph" => cmd_cce_graph_run(&ctx, &cli),
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
