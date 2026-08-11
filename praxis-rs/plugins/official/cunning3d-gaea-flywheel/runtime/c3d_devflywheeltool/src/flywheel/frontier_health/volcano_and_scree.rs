fn cmd_volcano_stage_parity(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "volcano-stage-parity",
        "Volcano",
        &["Volcano"],
        "gaea_volcano_stage_parity",
        &["case", "stage", "kind"],
        &["only-mismatch", "list-stages"],
    )
}

fn cmd_mapped_probe(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    default_node: &str,
    node_aliases: &[&str],
    bin: &str,
    value_flags: &[&str],
    switch_flags: &[&str],
) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or(default_node);
    if !node_aliases
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, command_name);
    }

    let mut command = probe_bin_command(ctx, cli, bin);
    pass_mapped_probe_flags(cli, &mut command, value_flags, switch_flags);
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print_allow_failure_artifact(ctx, cli, command_name, vec![command], None)
}

fn cmd_scree_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Scree");
    if !node.eq_ignore_ascii_case("Scree") {
        return command_not_wired(node, "scree-compare");
    }

    let dump_prefix = scree_dump_prefix(cli);
    let case_name = cli
        .flag("case")
        .map(str::to_string)
        .unwrap_or_else(|| dump_prefix.clone());
    let run_dir = ctx.artifact_root.join("scree-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));
    let explicit_bridge_dir = cli.flag("bridge-dir").map(PathBuf::from);
    let bridge_dir = explicit_bridge_dir
        .clone()
        .unwrap_or_else(|| run_dir.join("bridge"));
    let native_only = cli.has("native-only");

    let mut commands = Vec::new();
    if !native_only && explicit_bridge_dir.is_none() {
        if cli.run() && !ctx.harness_exe.exists() {
            return Err(format!(
                "GaeaReverseHarness executable not found at '{}'. Build it before running scree-compare without --bridge-dir.",
                ctx.harness_exe.display()
            ));
        }
        commands.push(scree_bridge_command(ctx, cli, &bridge_dir, &dump_prefix));
    }
    commands.push(scree_native_compare_command(
        ctx,
        cli,
        &bridge_dir,
        &dump_prefix,
    ));

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "scree-compare",
            "node": "Scree",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "bridge_dir": path_text(&bridge_dir),
            "prefix": dump_prefix,
            "native_only": native_only,
            "fresh_bridge_generation": !native_only && explicit_bridge_dir.is_none(),
            "commands": commands.iter().map(command_preview).collect::<Vec<_>>(),
            "truth_rule": if native_only {
                "Scree native-only mode is a performance profiler over synthetic input maps; use full scree-compare for Bridge/native raw stage parity."
            } else {
                "Scree Bridge stages from GaeaReverseHarness feed the Rust native stage compare; exact remains bitwise and passed may use --epsilon for float-only residuals."
            }
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    execute_or_print_allow_failure_artifact(ctx, cli, "scree-compare", commands, Some(run_dir))
}

fn scree_bridge_command(ctx: &Context, cli: &Cli, dump_dir: &Path, dump_prefix: &str) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-scree-connected-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.arg("--height-map");
    command.arg(scree_height_map_token(cli));
    if let Some(precipitation) = cli.flag("precipitation-map") {
        command.args(["--precipitation-map", precipitation]);
    }
    command.args([
        "--scale",
        cli.flag("scale").unwrap_or("0.6"),
        "--height",
        cli.flag("height").unwrap_or("1.0"),
        "--density",
        cli.flag("density").unwrap_or("1"),
        "--spread",
        cli.flag("spread").unwrap_or("0.0"),
        "--edge",
        cli.flag("edge").unwrap_or("0.4"),
        "--seed",
        cli.flag("seed").unwrap_or("0"),
        "--terrain-width",
        cli.flag("terrain-width").unwrap_or("1000"),
        "--terrain-height",
        cli.flag("terrain-height").unwrap_or("500"),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    command
}

fn scree_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    bridge_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_scree_bridge_native_compare");
    command.args([
        "--bridge-dir",
        bridge_dir.to_str().unwrap_or_default(),
        "--prefix",
        dump_prefix,
        "--source",
        cli.flag("source").unwrap_or("flat"),
        "--resolution",
        cli.flag("resolution").unwrap_or("16"),
        "--scale",
        cli.flag("scale").unwrap_or("0.6"),
        "--height",
        cli.flag("height").unwrap_or("1.0"),
        "--density",
        cli.flag("density").unwrap_or("1"),
        "--spread",
        cli.flag("spread").unwrap_or("0.0"),
        "--edge",
        cli.flag("edge").unwrap_or("0.4"),
        "--seed",
        cli.flag("seed").unwrap_or("0"),
    ]);
    if let Some(epsilon) = cli.flag("epsilon") {
        command.args(["--epsilon", epsilon]);
    }
    if let Some(repeat) = cli.flag("repeat") {
        command.args(["--repeat", repeat]);
    }
    if cli.has("native-only") {
        command.arg("--native-only");
    }
    if let Some(token) = cli.flag("height-map").or_else(|| cli.flag("input-map")) {
        command.args(["--height-map", token]);
    }
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    command
}

fn scree_height_map_token(cli: &Cli) -> String {
    if let Some(token) = cli.flag("height-map").or_else(|| cli.flag("input-map")) {
        return token.replace("{res}", cli.flag("resolution").unwrap_or("16"));
    }
    let resolution = cli.flag("resolution").unwrap_or("16");
    match cli
        .flag("source")
        .unwrap_or("flat")
        .to_ascii_lowercase()
        .as_str()
    {
        "cone" => format!("map:cone:{resolution}:1:0.47:0.53:0.42"),
        "rampy" | "ramp-y" => format!("map:rampy:{resolution}:0:1"),
        "checker" => format!("map:checker:{resolution}:0:1:8"),
        _ => format!("map:flat:{resolution}:0"),
    }
}

fn scree_dump_prefix(cli: &Cli) -> String {
    if let Some(prefix) = cli.flag("prefix") {
        return sanitize_filename(prefix);
    }
    sanitize_filename(&format!(
        "{}{}_scale{}_height{}_density{}_spread{}_edge{}_seed{}",
        cli.flag("source").unwrap_or("flat"),
        cli.flag("resolution").unwrap_or("16"),
        cli.flag("scale").unwrap_or("0.6"),
        cli.flag("height").unwrap_or("1.0"),
        cli.flag("density").unwrap_or("1"),
        cli.flag("spread").unwrap_or("0.0"),
        cli.flag("edge").unwrap_or("0.4"),
        cli.flag("seed").unwrap_or("0")
    ))
}
