fn cmd_erosion2_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Erosion2");
    if !["Erosion2", "Erosion2Node"]
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "erosion2-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_erosion2_bridge_native_compare");
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "mask",
            "epsilon",
            "matrix",
            "dump-dir",
            "duration",
            "downcutting",
            "erosion-scale",
            "suspended-amount",
            "suspended-angle",
            "bed-amount",
            "bed-angle",
            "coarse-amount",
            "coarse-angle",
            "shape",
            "shape-sharpness",
            "shape-detail-scale",
            "seed",
            "enable",
            "enable-orographic",
            "enable-orographic-influence",
            "directional-precipitation",
            "direction",
            "rain-shadow",
            "slope-min",
            "slope-max",
            "altitude-min",
            "altitude-max",
            "reverse",
            "require-speedup",
        ],
        &["require-all-pass", "require-exact"],
    );
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print(ctx, cli, "erosion2-compare", vec![command], None)
}

fn cmd_mask_flow_bridge_probe(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    default_node: &str,
    node_aliases: &[&str],
) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or(default_node);
    if !node_aliases
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, command_name);
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_mask_flow_bridge_probe");
    command.arg("--node");
    command.arg(node);
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "input-source",
            "input-map",
            "height-source",
            "height-map",
            "source",
            "layer-source",
            "layer-map",
            "base-source",
            "base-map",
            "mask-source",
            "mask-map",
            "scale",
            "height",
            "x",
            "y",
            "flatten",
            "direction",
            "edge",
            "min",
            "max",
            "range-min",
            "range-max",
            "falloff",
            "slope-type",
            "micro-accent",
            "flow-mode",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["require-all-pass", "require-pass"],
    );
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print_allow_failure_artifact(ctx, cli, command_name, vec![command], None)
}

fn cmd_ground_texture_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "ground-texture-bridge-probe",
        "GroundTexture",
        &["GroundTexture", "Ground Texture"],
        "gaea_ground_texture_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "method",
            "strength",
            "coverage",
            "density",
            "node-id",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["compare-native"],
    )
}
