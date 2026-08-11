fn probe_bin_command(ctx: &Context, cli: &Cli, bin: &str) -> Command {
    if cli.has("direct-bin") {
        let target_dir = if cli.prefers_release_probe_bins() {
            &ctx.cunning_core_target_release_dir
        } else {
            &ctx.cunning_core_target_debug_dir
        };
        let path = target_dir.join(format!("{bin}.exe"));
        if path.exists() && (cli.has("allow-stale-direct-bin") || probe_bin_is_fresh(ctx, &path)) {
            return Command::new(path);
        }
    }
    cargo_bin_command(ctx, cli, bin)
}

fn probe_bin_is_fresh(ctx: &Context, path: &Path) -> bool {
    let Ok(binary_modified) = path.metadata().and_then(|metadata| metadata.modified()) else {
        return false;
    };
    let roots = [
        ctx.root
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield"),
        ctx.root
            .join("crates")
            .join("cunning_core")
            .join("src")
            .join("bin"),
    ];
    roots
        .iter()
        .all(|root| source_tree_older_than(root, binary_modified))
}

fn source_tree_older_than(root: &Path, binary_modified: SystemTime) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return true;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            if !source_tree_older_than(&path, binary_modified) {
                return false;
            }
            continue;
        }
        let extension = path.extension().and_then(OsStr::to_str).unwrap_or("");
        if !matches!(extension, "rs" | "wgsl") {
            continue;
        }
        if metadata
            .modified()
            .map(|modified| modified > binary_modified)
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

fn is_allowed_engine_utility_class(class: &str) -> bool {
    matches!(
        class,
        "MapHelper" | "TileHelper" | "Transformer" | "ColorHelper"
    )
}

fn cargo_bin_command(ctx: &Context, cli: &Cli, bin: &str) -> Command {
    let mut command = Command::new("cargo");
    command.env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir);
    if cli.has("no-incremental") {
        command.env("CARGO_INCREMENTAL", "0");
    }
    command.args(["run"]);
    if cli.prefers_release_probe_bins() {
        command.arg("--release");
    }
    command.args([
        "--manifest-path",
        ctx.cunning_core_manifest.to_str().unwrap_or_default(),
    ]);
    if let Some(features) = cargo_bin_features(bin) {
        command.args(["--features", features]);
    }
    command.args(["--bin", bin, "--"]);
    command
}

fn cargo_bin_features(bin: &str) -> Option<&'static str> {
    match bin {
        "gaea_erosion_classic_substrate_probe" => Some("gaea_flywheel_probe"),
        "gaea_weathering_native_probe" => Some("gaea_flywheel_probe"),
        _ => None,
    }
}

fn cmd_probe_bin(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let bin = cli
        .flag("bin")
        .ok_or_else(|| "probe-bin requires --bin <provider_probe_bin>.".to_string())?;
    validate_parity_probe_bin(ctx, bin)?;
    let mut command = probe_bin_command(ctx, cli, bin);
    append_passthrough_args(&mut command, cli);
    let output_path = ctx
        .artifact_root
        .join("probe-bin")
        .join(sanitize_filename(bin))
        .join(unix_stamp_millis().to_string());
    execute_or_print_allow_failure_artifact(ctx, cli, "probe-bin", vec![command], Some(output_path))
}

fn cmd_harness_build(ctx: &Context, cli: &Cli) -> Result<(), String> {
    if !ctx.harness_project.is_file() {
        return Err(format!(
            "GaeaReverseHarness project does not exist at '{}'.",
            ctx.harness_project.display()
        ));
    }
    let mut command = Command::new("dotnet");
    command
        .arg("build")
        .arg(&ctx.harness_project)
        .arg("--nologo");
    execute_or_print_allow_failure_artifact(ctx, cli, "harness-build", vec![command], None)
}

fn validate_parity_probe_bin(ctx: &Context, bin: &str) -> Result<(), String> {
    let Some(provider) = parity_probe_provider(bin) else {
        return Err(format!(
            "probe-bin only accepts owned provider probes such as gaea_sandstone_bridge_probe or polybevel_blender_cube_compare; got '{bin}'."
        ));
    };
    let source = ctx
        .root
        .join("crates")
        .join("cunning_core")
        .join("src")
        .join("bin")
        .join(format!("{bin}.rs"));
    if !source.exists() {
        return Err(format!(
            "{provider} provider probe source '{}' does not exist.",
            source.display()
        ));
    }
    Ok(())
}
