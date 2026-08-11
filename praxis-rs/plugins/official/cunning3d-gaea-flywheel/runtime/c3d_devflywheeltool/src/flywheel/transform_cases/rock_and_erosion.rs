fn cmd_rock_core_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "rock-core-compare",
        "RockCore",
        &["RockCore", "Outcrops"],
        "gaea_rock_core_compare",
        &[
            "case",
            "matrix",
            "oracle-root",
            "epsilon",
            "repeat",
            "resolution",
            "source",
            "crumble-backend",
            "dump-dir",
        ],
        &[
            "require-all-pass",
            "require-exact",
            "native-only",
            "profile",
        ],
    )
}

fn cmd_rock_noise_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "rock-noise-compare",
        "RockNoise",
        &["RockNoise", "Rock Noise", "rock_noise"],
        "gaea_rock_noise_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "height-map",
            "size-x",
            "size-y",
            "variety",
            "octaves",
            "seed",
            "epsilon",
            "repeat",
            "target-speedup",
            "matrix",
            "dump-dir",
            "harness-exe",
        ],
        &["require-all-pass", "require-exact", "require-speedup"],
    )
}

fn cmd_easy_erosion_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "easy-erosion-compare",
        "EasyErosion",
        &["EasyErosion", "Easy Erosion"],
        "gaea_easy_erosion_bridge_native_compare",
        &[
            "resolution",
            "case",
            "label",
            "epsilon",
            "repeat",
            "target-speedup",
            "matrix",
        ],
        &[
            "require-all-pass",
            "require-exact",
            "require-speedup",
            "dump-native-stages",
            "list-cases",
        ],
    )
}

fn cmd_rugged_stage_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "rugged-stage-compare",
        "Rugged",
        &["Rugged"],
        "gaea_rugged_m3_stage_bridge_native_compare",
        &[
            "surface",
            "resolution",
            "terrain-width",
            "terrain-height",
            "scale",
            "seed",
            "epsilon",
            "repeat",
            "matrix",
            "target-speedup",
            "harness-exe",
            "dump-root",
            "dump-dir",
        ],
        &[
            "require-pass",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_hydro_fix_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "hydro-fix-bridge-probe",
        "HydroFix",
        &["HydroFix", "Hydro Fix"],
        "gaea_hydro_fix_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "downcutting",
            "epsilon",
        ],
        &["compare-native"],
    )
}
