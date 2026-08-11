fn fractal_terrace_internal_cases(cli: &Cli) -> Result<Vec<FractalTerraceInternalCase>, String> {
    if let Some(matrix) = cli.flag("matrix") {
        if matrix.eq_ignore_ascii_case("focused") {
            return Ok(fractal_terrace_internal_focused_cases());
        }
        if matches!(
            matrix.to_ascii_lowercase().as_str(),
            "production" | "prod" | "expanded" | "wide"
        ) {
            return Ok(fractal_terrace_internal_production_cases());
        }
        return Err(format!(
            "Unknown FractalTerraces internals matrix '{matrix}'. Supported matrices: focused, production."
        ));
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(32).max(2);
    let input_map = cli
        .flag("input-map")
        .or_else(|| cli.flag("map"))
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:cone:{resolution}:1:0.5:0.5:0.45"));
    Ok(vec![FractalTerraceInternalCase {
        name: cli.case_name(),
        input_map,
        resolution,
        spacing: optional_f32_flag(cli, "spacing")?.unwrap_or(0.1),
        octaves: optional_usize_flag(cli, "octaves")?.unwrap_or(12),
        intensity: optional_f32_flag(cli, "intensity")?.unwrap_or(0.5),
        shape: optional_f32_flag(cli, "shape")?.unwrap_or(0.0),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or(0),
        tilt_amount: optional_f32_flag(cli, "tilt-amount")?.unwrap_or(0.5),
        tilt_seed: optional_i32_flag(cli, "tilt-seed")?.unwrap_or(-1),
        direction: optional_i32_flag(cli, "direction")?.unwrap_or(0),
    }])
}

fn fractal_terrace_internal_focused_cases() -> Vec<FractalTerraceInternalCase> {
    vec![
        fractal_terrace_internal_case(
            "default_cone_32",
            "map:cone:32:1:0.5:0.5:0.45",
            32,
            0.1,
            12,
            0.5,
            0.0,
            0,
            0.5,
            -1,
            0,
        ),
        fractal_terrace_internal_case(
            "rampx_shape_pos_32",
            "map:rampx:32:0.02:0.92",
            32,
            0.07,
            8,
            0.75,
            0.4,
            777,
            0.8,
            12345,
            35,
        ),
        fractal_terrace_internal_case(
            "rampy_shape_neg_64",
            "map:rampy:64:0.03:0.97",
            64,
            0.12,
            12,
            0.65,
            -0.35,
            -42,
            0.3,
            98765,
            125,
        ),
        fractal_terrace_internal_case(
            "checker_low_octaves_32",
            "map:checker:32:0.1:0.9:5",
            32,
            0.18,
            3,
            0.25,
            0.8,
            21,
            1.0,
            5,
            270,
        ),
        fractal_terrace_internal_case(
            "radial_dense_64",
            "map:radial:64:1:0:0.5:0.5:0.42",
            64,
            0.035,
            12,
            1.0,
            -0.75,
            1357,
            0.65,
            -2468,
            315,
        ),
        fractal_terrace_internal_case(
            "sine_mid_64",
            "map:sine:64:7:0.25:0.45",
            64,
            0.09,
            6,
            0.55,
            0.15,
            2024,
            0.45,
            2025,
            80,
        ),
    ]
}

fn fractal_terrace_internal_production_cases() -> Vec<FractalTerraceInternalCase> {
    let mut cases = fractal_terrace_internal_focused_cases();
    cases.extend([
        fractal_terrace_internal_case(
            "rampx_high_res_extreme_shape_128",
            "map:rampx:128:0.01:0.99",
            128,
            0.04,
            12,
            1.0,
            1.0,
            101,
            1.0,
            111,
            359,
        ),
        fractal_terrace_internal_case(
            "corner_impulse_tilt_64",
            "map:impulse:64:1:0:0",
            64,
            0.04,
            12,
            1.0,
            -1.0,
            -777,
            1.0,
            111,
            0,
        ),
        fractal_terrace_internal_case(
            "edge_impulse_sparse_64",
            "map:impulse:64:1:63:0",
            64,
            0.001,
            1,
            0.2,
            1.0,
            42,
            0.0,
            0,
            90,
        ),
        fractal_terrace_internal_case(
            "sine_midfreq_96",
            "map:sine:96:9:0.31:0.48",
            96,
            0.11,
            9,
            0.9,
            0.65,
            -909,
            0.6,
            2026,
            225,
        ),
        fractal_terrace_internal_case(
            "checker_fine_128",
            "map:checker:128:0:1:1",
            128,
            1.0,
            12,
            1.0,
            -0.95,
            4242,
            0.25,
            5150,
            180,
        ),
        fractal_terrace_internal_case(
            "flat_zero_tilt_32",
            "map:flat:32:0.5",
            32,
            0.33,
            3,
            0.0,
            0.25,
            -17,
            0.0,
            0,
            0,
        ),
        fractal_terrace_internal_case(
            "radial_offcenter_128",
            "map:radial:128:0.9:0.1:0.2:0.8:0.7",
            128,
            0.22,
            5,
            0.35,
            -0.6,
            -202,
            0.95,
            77,
            90,
        ),
        fractal_terrace_internal_case(
            "cone_offcenter_64",
            "map:cone:64:0.8:0.15:0.85:0.2",
            64,
            0.18,
            10,
            0.95,
            -0.85,
            9090,
            0.7,
            -303,
            45,
        ),
    ]);
    cases
}

#[allow(clippy::too_many_arguments)]
fn fractal_terrace_internal_case(
    name: &str,
    input_map: &str,
    resolution: u32,
    spacing: f32,
    octaves: usize,
    intensity: f32,
    shape: f32,
    seed: i32,
    tilt_amount: f32,
    tilt_seed: i32,
    direction: i32,
) -> FractalTerraceInternalCase {
    FractalTerraceInternalCase {
        name: name.to_string(),
        input_map: input_map.to_string(),
        resolution: resolution.max(2),
        spacing,
        octaves,
        intensity,
        shape,
        seed,
        tilt_amount,
        tilt_seed,
        direction,
    }
}
