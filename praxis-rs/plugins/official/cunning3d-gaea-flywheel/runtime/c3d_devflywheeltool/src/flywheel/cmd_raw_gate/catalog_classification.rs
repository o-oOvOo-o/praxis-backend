fn classify_public_node_kind(
    text: &str,
    input_ports: &[FlywheelPort],
    output_ports: &[FlywheelPort],
) -> &'static str {
    let generator =
        text.contains("Classification.Generator") || text.contains("NodeCategory.Terrain");
    let multi_output = output_ports.len() > 1;
    if generator && multi_output {
        "generator_multi_output"
    } else if generator {
        "generator"
    } else if input_ports.is_empty() && multi_output {
        "source_multi_output"
    } else if input_ports.is_empty() {
        "source_or_utility"
    } else if multi_output {
        "connected_operator_multi_output"
    } else {
        "connected_operator"
    }
}

fn candidate_priority(id: &str) -> &'static str {
    match id {
        "Mountain" | "Canyon" | "EasyErosion" | "Erosion" | "Erosion2" => "critical",
        "MountainRange" | "Volcano" | "Ridge" | "Perlin" | "Voronoi" | "MultiFractal"
        | "River2" | "Rivers" => "high",
        "Thermal" | "Thermal2" | "DuneSea" | "Glacier" | "Island" | "CraterField" => "medium",
        _ => "low",
    }
}

fn contract_id_for_call(class: &str, method: &str) -> String {
    mapped_contract_id(class, method).unwrap_or_else(|| {
        format!(
            "blackbox.{}.{}",
            class.to_ascii_lowercase(),
            method.to_ascii_lowercase()
        )
    })
}

fn mapped_contract_id(class: &str, method: &str) -> Option<String> {
    let key = format!(
        "{}.{}",
        class.to_ascii_lowercase(),
        method.to_ascii_lowercase()
    );
    match key.as_str() {
        "landscapes.mountain" => Some("mountain.recipe".to_string()),
        "landscapes.canyon" => Some("canyon.recipe".to_string()),
        "erosions.pe" => Some("erosions.pe.public_shell".to_string()),
        "erosions.classic" => Some("erosions.classic.wrapper".to_string()),
        "profiles.complexterraces" | "profiles.fractalterrace" => {
            Some("fractal_terrace.height_path".to_string())
        }
        "combiner.min" | "combiner.max" => Some("combiner.minmax_height_shell".to_string()),
        "combiner.subtract" => Some("combiner.subtract_ratio_mix".to_string()),
        "gradients.lineargradient" => Some("gradient.linear_bias_overlay".to_string()),
        "rockcore.noise" => Some("rockcore.noise.overlay".to_string()),
        "warps.fractalwarp" => Some("fractal_warp.virtual_identity_sampling".to_string()),
        "noises.voronoi" => Some("voronoi.raw_substrate".to_string()),
        _ => None,
    }
}

fn layer_for_class(class: &str) -> &'static str {
    match class {
        "Combiner" | "ClassicCombiner" | "MapHelper" | "FMath" | "Masking" | "AspectMaps" => "L0",
        "Noises" | "RandomNoises" | "Gradients" | "Profiles" | "Warps" | "Others" | "Morph"
        | "Surfacer" | "Surfaces" | "Texturize" | "SlopeBlurCore" | "RawNoise" | "FilterCore"
        | "Morphology" | "Morphology2" | "MorphologyRT" | "WarpField" => "L1",
        "Erosions" | "Simulations" | "Waters" | "Scatters" | "RockCore" | "DebrisCore"
        | "FacetedRock" | "HybridBlender" | "VectorMask" | "Lighting2" => "L2",
        "Landscapes" => "L4",
        _ => "L1",
    }
}

fn operator_family_for_class(class: &str) -> &'static str {
    match class {
        "Erosions" | "Simulations" | "Waters" => "erosion/water/simulation",
        "Landscapes" => "landscape recipe",
        "Noises" | "RandomNoises" | "RawNoise" => "noise",
        "Gradients" | "Profiles" => "profile/gradient",
        "Combiner" | "ClassicCombiner" | "Masking" | "MapHelper" => "map composition",
        "Warps" | "WarpField" => "warp",
        "Surfacer" | "Surfaces" | "Texturize" => "surface/material",
        "Scatters" | "RockCore" | "DebrisCore" | "FacetedRock" => "rock/scatter",
        _ => "shared substrate",
    }
}

fn priority_rank_text(priority: &str) -> u8 {
    match priority {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup_by(|lhs, rhs| lhs.eq_ignore_ascii_case(rhs));
    values
}

fn push_unique_string(values: &mut Vec<String>, value: &str) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
}

fn operator_key(class: &str, method: &str) -> String {
    format!("{class}.{method}")
}

fn gaea_nodes_source_dir(ctx: &Context) -> PathBuf {
    ctx.gaea_decompiled_root
        .join("Gaea.Nodes")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Nodes")
}

fn resolve_node_source_path(ctx: &Context, file: &str) -> PathBuf {
    let mut normalized = file.replace('/', "\\");
    if let Some(stripped) = normalized.strip_prefix("Nodes\\") {
        normalized = stripped.to_string();
    }
    gaea_nodes_source_dir(ctx).join(normalized)
}

fn resolve_operator_source_path(ctx: &Context, method: &CatalogOperatorMethod) -> PathBuf {
    let direct = PathBuf::from(&method.file);
    if direct.is_absolute() && direct.exists() {
        return direct;
    }
    if !method.file.is_empty() {
        let node_path = resolve_node_source_path(ctx, &method.file);
        if node_path.exists() {
            return node_path;
        }
        let core_path = gaea_nodes_source_dir(ctx).join("Core").join(&method.file);
        if core_path.exists() {
            return core_path;
        }
        for engine_subdir in ["Processing", "Utilities"] {
            let engine_path = gaea_engine_source_dir(ctx)
                .join(engine_subdir)
                .join(&method.file);
            if engine_path.exists() {
                return engine_path;
            }
        }
    }
    source_file_for_class(ctx, &method.class).unwrap_or_else(|| gaea_nodes_source_dir(ctx))
}

fn source_file_for_class(ctx: &Context, class: &str) -> Option<PathBuf> {
    let nodes_dir = gaea_nodes_source_dir(ctx);
    let engine_dir = gaea_engine_source_dir(ctx);
    let candidates = [
        nodes_dir.join(format!("{class}.cs")),
        nodes_dir.join("Core").join(format!("{class}.cs")),
        engine_dir.join("Processing").join(format!("{class}.cs")),
        engine_dir.join("Utilities").join(format!("{class}.cs")),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn gaea_engine_source_dir(ctx: &Context) -> PathBuf {
    ctx.gaea_decompiled_root
        .join("Gaea.Engine")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Engine")
}
