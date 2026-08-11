fn cmd_canyon_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Canyon") {
        return command_not_wired(&node, "canyon-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_canyon_bridge_native_compare");
    pass_canyon_compare_flags(cli, &mut command);
    if cli.json() {
        command.arg("--json");
    }
    execute_or_print(ctx, cli, "canyon-compare", vec![command], None)
}

fn pass_canyon_compare_flags(cli: &Cli, command: &mut Command) {
    for key in [
        "resolution",
        "terrain-width",
        "terrain-height",
        "style",
        "scale",
        "slot",
        "valley",
        "surrounding",
        "depth",
        "structural-warp",
        "detail-warp",
        "alternate-style",
        "seed",
        "epsilon",
        "dump-dir",
        "matrix",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
}

fn cmd_mountain_side_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("MountainSide") && !node.eq_ignore_ascii_case("Mountain Side") {
        return command_not_wired(&node, "mountain-side-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_side_native_self_compare");
    pass_mountain_side_compare_flags(cli, &mut command);
    if cli.json() {
        command.arg("--json");
    }
    execute_or_print(ctx, cli, "mountain-side-compare", vec![command], None)
}

fn pass_mountain_side_compare_flags(cli: &Cli, command: &mut Command) {
    for key in [
        "resolution",
        "stage-resolution",
        "warmup",
        "repeat",
        "epsilon",
        "matrix",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            if key == "matrix" && value.eq_ignore_ascii_case("full-promotion") {
                command.arg("all");
            } else {
                command.arg(value);
            }
        }
    }
    if let Some(value) = cli.flag("require-speedup") {
        command.arg("--require-speedup");
        command.arg(value);
    } else if cli.has("require-speedup") {
        command.arg("--require-speedup");
    }
}

fn cmd_ridge_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "ridge-compare",
        "Ridge",
        &["Ridge"],
        "gaea_ridge_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "scale",
            "height",
            "definition",
            "seed",
            "scale-x",
            "scale-y",
            "repeat",
            "sweep",
            "sweep-seed",
        ],
        &[
            "require-exact",
            "require-all-pass",
            "require-accepted",
            "native-only",
        ],
    )
}

fn cmd_stratify_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "stratify-compare",
        "Stratify",
        &["Stratify"],
        "gaea_stratify_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "input-map",
            "spacing",
            "octaves",
            "intensity",
            "shape",
            "seed",
            "tilt-amount",
            "direction",
            "sweep",
            "sweep-seed",
            "repeat",
        ],
        &["require-exact", "require-accepted", "native-only"],
    )
}

fn cmd_crater_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "crater-compare",
        "Crater",
        &["Crater"],
        "gaea_crater_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "style",
            "scale",
            "formation",
            "height",
            "rim",
            "shape",
            "seed",
            "x",
            "y",
            "sweep",
            "classic-sweep",
            "sweep-seed",
            "repeat",
            "target-speedup",
            "dump-dir",
        ],
        &[
            "require-all-pass",
            "require-exact",
            "require-accepted",
            "native-only",
            "classic-stage-report",
            "require-speedup",
            "require-speedup-gate",
        ],
    )
}

fn cmd_sand_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "sand-compare",
        "Sand",
        &["Sand"],
        "gaea_sand_bridge_native_compare",
        &[
            "matrix",
            "resolution",
            "scale",
            "direction",
            "chaos",
            "softness",
            "height",
            "warp-by-terrain",
            "seed",
            "input-map",
            "terrain-width",
            "terrain-height",
            "epsilon",
            "repeat",
            "dump-dir",
            "bridge-dump-dir",
            "harness-exe",
        ],
        &["reuse-dumps", "require-pass", "require-exact"],
    )
}

fn cmd_craterfield_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "craterfield-compare",
        "CraterField",
        &["CraterField", "Craterfield", "Crater Field"],
        "gaea_craterfield_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "scale",
            "depth",
            "density",
            "seed",
            "x",
            "y",
            "warp-row",
            "repeat",
            "sweep",
            "sweep-seed",
        ],
        &[
            "require-exact",
            "require-accepted",
            "native-only",
            "profile-native",
        ],
    )
}

fn cmd_transform_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    if cli.has("matrix") {
        return cmd_transform_compare_matrix(ctx, cli);
    }
    cmd_mapped_probe(
        ctx,
        cli,
        "transform-compare",
        "Transform",
        &["Transform"],
        "gaea_transform_bridge_mountain_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "mountain-scale",
            "mountain-height",
            "mountain-style",
            "mountain-bulk",
            "seed",
            "offset-x",
            "offset-y",
            "offset-z",
            "uniform",
            "scale",
            "scale-x",
            "scale-y",
            "rotate",
            "blend-mode",
            "edges",
            "quality",
            "base-map",
            "epsilon",
            "dump-dir",
        ],
        &[],
    )
}
