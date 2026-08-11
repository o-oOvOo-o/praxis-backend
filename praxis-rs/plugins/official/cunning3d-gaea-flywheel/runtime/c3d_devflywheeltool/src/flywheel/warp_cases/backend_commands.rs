fn mountain_backend_compare_command(
    ctx: &Context,
    cli: &Cli,
    case_name: &str,
    json: bool,
    audit: bool,
    worst_cell: bool,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_backend_compare");
    command.args([
        "--case",
        case_name,
        "--lhs",
        "native_live",
        "--rhs",
        "gaea_bridge",
    ]);
    if json {
        command.arg("--json");
    }
    if audit {
        command.arg("--enforce-smoke-limits");
        command.arg("--require-exact");
    }
    if worst_cell {
        command.arg("--worst-cell-diagnostics");
        command.arg("--aux-diagnostics");
    }
    command
}

fn thermal2_bridge_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    case_name: &str,
    audit: bool,
    first: bool,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_thermal2_bridge_native_compare");
    command.arg("--json");
    if let Some(matrix) = cli.flag("matrix") {
        command.args(["--matrix", matrix]);
    } else if audit && case_name.eq_ignore_ascii_case("all") {
        command.args(["--matrix", "focused"]);
    } else {
        command.args(["--case", case_name]);
    }
    if audit {
        command.arg("--require-exact");
    }
    if first {
        command.arg("--first");
    }
    command.arg("--harness-exe").arg(&ctx.harness_exe);
    for key in [
        "map",
        "area",
        "area-mask",
        "sediment-removal-map",
        "sediment-removal-mask",
        "terrain-width",
        "terrain-height",
        "duration",
        "strength",
        "anisotropy",
        "angle",
        "talus-angle",
        "feature-scale",
        "erosion-scale",
        "sediment-removal",
        "use-area-mask",
        "use-sediment-removal-mask",
        "epsilon",
        "repeat",
        "dump-root",
        "gaea-dir",
    ] {
        append_optional_arg(&mut command, cli, key);
    }
    command
}

fn thermal2_bridge_probe_command(
    ctx: &Context,
    cli: &Cli,
    case_name: &str,
    run_dir: &Path,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-thermal2");
    maybe_add_gaea_dir(cli, &mut command);
    let case = thermal2_bridge_probe_case(case_name).unwrap_or_else(|error| {
        panic!("Thermal2 bridge probe case resolution failed: {error}");
    });
    command.arg("--map");
    command.arg(cli.flag("map").unwrap_or(case.map.as_str()));
    if let Some(value) = cli.flag("area").or_else(|| cli.flag("area-mask")) {
        command.arg("--area");
        command.arg(value);
    } else if let Some(area) = case.area_mask.as_deref() {
        command.arg("--area");
        command.arg(area);
    }
    if let Some(value) = cli
        .flag("sediment-removal-map")
        .or_else(|| cli.flag("sediment-removal-mask"))
    {
        command.arg("--sediment-removal-map");
        command.arg(value);
    } else if let Some(sediment) = case.sediment_removal_map.as_deref() {
        command.arg("--sediment-removal-map");
        command.arg(sediment);
    }
    command.arg("--terrain-width");
    command.arg(
        cli.flag("terrain-width")
            .unwrap_or(case.terrain_width.as_str()),
    );
    command.arg("--terrain-height");
    command.arg(
        cli.flag("terrain-height")
            .unwrap_or(case.terrain_height.as_str()),
    );
    command.arg("--duration");
    command.arg(cli.flag("duration").unwrap_or(case.duration.as_str()));
    command.arg("--strength");
    command.arg(cli.flag("strength").unwrap_or(case.strength.as_str()));
    command.arg("--anisotropy");
    command.arg(cli.flag("anisotropy").unwrap_or(case.anisotropy.as_str()));
    command.arg("--angle");
    command.arg(
        cli.flag("angle")
            .or_else(|| cli.flag("talus-angle"))
            .unwrap_or(case.angle.as_str()),
    );
    command.arg("--feature-scale");
    command.arg(
        cli.flag("feature-scale")
            .or_else(|| cli.flag("erosion-scale"))
            .unwrap_or(case.feature_scale.as_str()),
    );
    command.arg("--sediment-removal");
    command.arg(
        cli.flag("sediment-removal")
            .unwrap_or(case.sediment_removal.as_str()),
    );
    command.arg("--use-area-mask");
    command.arg(cli.flag("use-area-mask").unwrap_or(if case.use_area_mask {
        "true"
    } else {
        "false"
    }));
    command.arg("--use-sediment-removal-mask");
    command.arg(cli.flag("use-sediment-removal-mask").unwrap_or(
        if case.use_sediment_removal_mask {
            "true"
        } else {
            "false"
        },
    ));
    command.arg("--dump-dir");
    command.arg(run_dir.to_str().unwrap_or_default());
    command.arg("--dump-prefix");
    command.arg(sanitize_filename(case_name));
    command
}
