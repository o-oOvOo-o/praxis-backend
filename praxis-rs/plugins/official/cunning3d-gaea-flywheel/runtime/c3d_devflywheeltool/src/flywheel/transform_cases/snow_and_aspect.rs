fn cmd_snow_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let connected = matches!(
        cli.command.as_str(),
        "snow-mountain-connected-probe" | "snow-connected-mountain-probe"
    );
    let command_name = if connected {
        "snow-mountain-connected-probe"
    } else {
        "snow-bridge-probe"
    };
    let node = cli.flag("node").unwrap_or("Snow");
    if !node.eq_ignore_ascii_case("Snow") {
        return command_not_wired(node, command_name);
    }

    let value_flags = [
        "resolution",
        "terrain-width",
        "terrain-height",
        "source",
        "height-input-json",
        "snow-input-json",
        "melt-input-json",
        "mountain-scale",
        "mountain-height",
        "mountain-style",
        "mountain-bulk",
        "seed",
        "duration",
        "intensity",
        "settle-thaw",
        "melt",
        "snow-line",
        "real-scale",
        "terrain-scale",
        "verticality",
        "slip-off-angle",
        "adhered-snow-mass",
        "model-scale",
        "epsilon",
        "diagnostics-dir",
        "dump-dir",
        "matrix",
        "target-speedup",
    ];
    let switch_flags = [
        "mountain-bridge-input",
        "fresh-bridge-cache",
        "compare-native",
        "require-all-pass",
        "require-exact",
        "require-speedup",
    ];
    let mut command = probe_bin_command(ctx, cli, "gaea_snow_bridge_probe");
    pass_mapped_probe_flags(cli, &mut command, &value_flags, &switch_flags);
    if connected && !cli.has("mountain-bridge-input") {
        command.arg("--mountain-bridge-input");
    }
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print_allow_failure_artifact(ctx, cli, command_name, vec![command], None)
}

fn cmd_snowfield_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "snowfield-bridge-probe",
        "Snowfield",
        &["Snowfield", "SnowField"],
        "gaea_snowfield_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "cascades",
            "duration",
            "intensity",
            "settle-thaw",
            "melt",
            "snow-line",
            "slip-off-angle",
            "adhered-snow-mass",
            "flows",
            "flows-depth",
            "seed",
            "sharp-buildup",
            "alternate-snowfall",
            "surface-details",
            "direction",
            "epsilon",
            "target-speedup",
            "diagnostics-dir",
            "dump-dir",
            "matrix",
        ],
        &[
            "compare-native",
            "stage-diagnostics",
            "fresh-bridge-cache",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_glacier_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "glacier-bridge-probe",
        "Glacier",
        &["Glacier"],
        "gaea_glacier_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "reference-source",
            "mountain-scale",
            "mountain-height",
            "mountain-style",
            "mountain-bulk",
            "mountain-seed",
            "scale",
            "scale2",
            "thickness",
            "height",
            "direction",
            "breakage",
            "rough-edges",
            "seed",
            "chipped",
            "secondary-breakage",
            "diagonal-breakage",
            "diagonal-breakage-direction",
            "breakage-count",
            "flow-breakage",
            "extreme",
            "flow-breakage-depth",
            "substructure",
            "substructure-density",
            "substructure-depth",
            "epsilon",
            "target-speedup",
            "dump-dir",
            "matrix",
        ],
        &[
            "mountain-bridge-input",
            "compare-native",
            "compare-stages",
            "fresh-bridge-cache",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_aspect_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Height");
    let operator = cli.flag("operator").unwrap_or_else(|| {
        if node.eq_ignore_ascii_case("Slope") {
            "slope"
        } else if node.eq_ignore_ascii_case("Angle") {
            "angle"
        } else if node.eq_ignore_ascii_case("Curvature") {
            "curvature"
        } else {
            "height"
        }
    });
    if ![
        "Aspect",
        "Height",
        "Slope",
        "Angle",
        "Curvature",
        "AspectMaps",
    ]
    .iter()
    .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "aspect-bridge-probe");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_aspect_bridge_probe");
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "mode",
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "source-token",
            "min",
            "max",
            "falloff",
            "azimuth",
            "micro-accent",
            "slope-type",
            "curvature-type",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["require-pass"],
    );
    if cli.flag("operator").is_none() {
        command.arg("--operator");
        command.arg(operator);
    } else if let Some(operator) = cli.flag("operator") {
        command.arg("--operator");
        command.arg(operator);
    }
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print(ctx, cli, "aspect-bridge-probe", vec![command], None)
}
