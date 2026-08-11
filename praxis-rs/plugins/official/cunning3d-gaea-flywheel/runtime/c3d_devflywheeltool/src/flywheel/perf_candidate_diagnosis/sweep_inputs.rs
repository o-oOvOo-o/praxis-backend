impl MountainSweepParams {
    fn to_json(&self) -> Value {
        json!({
            "style": self.style,
            "bulk": self.bulk,
            "reduce_details": self.reduce_details,
            "scale": self.scale,
            "height": self.height,
            "seed": self.seed,
            "x": self.x,
            "y": self.y,
            "terrain_width": self.terrain_width,
            "terrain_height": self.terrain_height,
            "resolution": self.resolution,
        })
    }
}

#[derive(Clone, Debug)]
struct SweepRng {
    state: u64,
}

impl SweepRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }

    fn range_i32(&mut self, min: i32, max: i32) -> i32 {
        min + (self.next_u32() % ((max - min + 1) as u32)) as i32
    }

    fn choose<'a>(&mut self, values: &'a [&'a str]) -> &'a str {
        values[(self.next_u32() as usize) % values.len()]
    }
}

fn mountain_sweep_params(
    cli: &Cli,
    rng: &mut SweepRng,
    index: usize,
) -> Result<MountainSweepParams, String> {
    const BULKS: &[&str] = &["low", "medium", "high"];
    let styles = style_choices(cli)?;
    let resolution_choices = resolution_choices(cli)?;
    Ok(MountainSweepParams {
        index,
        style: cli
            .flag("style")
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| styles[(rng.next_u32() as usize) % styles.len()].clone()),
        bulk: cli
            .flag("bulk")
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| rng.choose(BULKS).to_string()),
        reduce_details: optional_bool_flag(cli, "reduce-details")?
            .unwrap_or_else(|| rng.next_u32() & 1 == 1),
        scale: optional_f32_flag(cli, "scale")?.unwrap_or_else(|| rng.range_f32(0.01, 2.0)),
        height: optional_f32_flag(cli, "height")?.unwrap_or_else(|| rng.range_f32(0.0, 10.0)),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or_else(|| rng.range_i32(0, 1_000_000)),
        x: optional_f32_flag(cli, "x")?.unwrap_or_else(|| rng.range_f32(0.0, 1.0)),
        y: optional_f32_flag(cli, "y")?.unwrap_or_else(|| rng.range_f32(0.0, 1.0)),
        terrain_width: optional_f32_flag(cli, "terrain-width")?
            .unwrap_or_else(|| rng.range_f32(1.0, 4096.0)),
        terrain_height: optional_f32_flag(cli, "terrain-height")?
            .unwrap_or_else(|| rng.range_f32(1.0, 4096.0)),
        resolution: optional_u32_flag(cli, "resolution")?.unwrap_or_else(|| {
            resolution_choices[(rng.next_u32() as usize) % resolution_choices.len()]
        }),
    })
}

fn mountain_candidate_sweep_params(
    cli: &Cli,
    rng: &mut SweepRng,
    index: usize,
    style_cycle: &[String],
) -> Result<MountainSweepParams, String> {
    let mut params = mountain_sweep_params(cli, rng, index)?;
    if cli.flag("style").is_none() {
        params.style = style_cycle[index % style_cycle.len()].clone();
    }
    Ok(params)
}

fn style_choices(cli: &Cli) -> Result<Vec<String>, String> {
    const DEFAULT_STYLES: &[&str] = &["basic", "eroded", "old", "alpine", "strata"];
    let source = cli.flag("style-choices");
    let mut values = Vec::new();
    match source {
        Some(text) => {
            for item in text.split(',') {
                let value = item.trim().to_ascii_lowercase();
                if !value.is_empty() {
                    values.push(value);
                }
            }
        }
        None => {
            values.extend(DEFAULT_STYLES.iter().map(|value| (*value).to_string()));
        }
    }
    if values.is_empty() {
        return Err("--style-choices must contain at least one style".to_string());
    }
    Ok(values)
}

fn resolution_choices(cli: &Cli) -> Result<Vec<u32>, String> {
    let text = cli.flag("resolution-choices").unwrap_or("256");
    let mut values = Vec::new();
    for item in text.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        values.push(
            trimmed
                .parse::<u32>()
                .map_err(|_| format!("--resolution-choices contains invalid integer '{trimmed}'"))?
                .max(2),
        );
    }
    if values.is_empty() {
        return Err("--resolution-choices must contain at least one integer".to_string());
    }
    Ok(values)
}
