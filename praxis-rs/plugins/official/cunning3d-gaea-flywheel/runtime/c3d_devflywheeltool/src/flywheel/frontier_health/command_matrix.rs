fn frontier_health_commands(
    ctx: &Context,
    cli: &Cli,
    suite: &str,
) -> Result<Vec<(String, Command)>, String> {
    let (include_frontier, include_foundation) = match suite {
        "quick" => (true, false),
        "foundation" => (false, true),
        "frontier" | "all" => (true, true),
        other => {
            return Err(format!(
                "Unknown frontier-health suite '{other}'. Use quick, foundation, frontier, or all."
            ));
        }
    };
    let epsilon = cli.flag("epsilon").unwrap_or("0");
    let mut commands = Vec::new();
    if include_frontier {
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_focused",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "focused",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_surrounding_no_coastal",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "surrounding-no-coastal",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_coastal_diagnostic",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "coastal-diagnostic",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_full_promotion",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "full-promotion",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "flow_map_focused",
            "gaea_flow_map_bridge_probe",
            &[
                "--matrix",
                "focused",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "gabor_focused",
            "gaea_gabor_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "hydro_fix_checker16",
            "gaea_hydro_fix_bridge_probe",
            &[
                "--resolution",
                "16",
                "--source",
                "checker",
                "--downcutting",
                "0.5",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "lake_basin16",
            "gaea_lake_bridge_probe",
            &[
                "--resolution",
                "16",
                "--source",
                "basin",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "snow_cone8",
            "gaea_snow_bridge_probe",
            &[
                "--resolution",
                "8",
                "--source",
                "cone",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "snowfield_cone8",
            "gaea_snowfield_bridge_probe",
            &[
                "--resolution",
                "8",
                "--source",
                "cone",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "glacier_cone8_radial_ref",
            "gaea_glacier_bridge_probe",
            &[
                "--resolution",
                "8",
                "--source",
                "cone",
                "--reference-source",
                "radial",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "fractal_terrace_internals",
            "gaea_fractal_terrace_internal_compare",
            &["--json"],
        );
    }
    if include_foundation {
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "graphic_eq_focused",
            "gaea_graphic_eq_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "deflate_focused",
            "gaea_deflate_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "denoise_focused",
            "gaea_denoise_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "peaks_focused",
            "gaea_peaks_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "uplift_focused",
            "gaea_uplift_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sharpen_focused",
            "gaea_sharpen_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "aspect_height_focused",
            "gaea_aspect_bridge_probe",
            &[
                "--mode",
                "compare",
                "--operator",
                "height",
                "--matrix",
                "focused",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "ground_texture_focused",
            "gaea_ground_texture_bridge_probe",
            &[
                "--matrix",
                "focused",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "recurve_focused",
            "gaea_recurve_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "rock_map_cone32",
            "gaea_rock_map_bridge_probe",
            &[
                "--resolution",
                "32",
                "--source",
                "cone",
                "--coverage",
                "0.5",
                "--density",
                "0.5",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "canyon_focused",
            "gaea_canyon_bridge_native_compare",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "erosion2_cone16",
            "gaea_erosion2_bridge_native_compare",
            &[
                "--resolution",
                "16",
                "--source",
                "cone",
                "--mask",
                "none",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "crater_new_smoke",
            "gaea_crater_bridge_native_compare",
            &[
                "--resolution",
                "32",
                "--scale",
                "0.5",
                "--formation",
                "0.5",
                "--height",
                "0.5",
                "--seed",
                "42",
                "--json",
            ],
        );
    }
    Ok(commands)
}
