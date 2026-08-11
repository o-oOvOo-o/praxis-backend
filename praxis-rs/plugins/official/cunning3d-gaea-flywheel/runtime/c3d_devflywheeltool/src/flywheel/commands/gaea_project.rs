fn cmd_gaea_project(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let preset = cli.flag("preset").unwrap_or("volcano-snow-material");
    if !matches!(
        preset,
        "volcano-snow-material" | "volcano-snow" | "snowy-volcano"
    ) {
        return Err(format!(
            "Unsupported Gaea project preset '{preset}'. Supported: volcano-snow-material."
        ));
    }
    let gaea_dir = cli
        .flag("gaea-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"F:\Gaea 2"));
    let template = cli
        .flag("template")
        .map(PathBuf::from)
        .unwrap_or_else(|| gaea_dir.join("Examples").join("Detailed Snow Peak.terrain"));
    let output = cli.flag("output").map(PathBuf::from).unwrap_or_else(|| {
        ctx.artifact_root
            .join("gaea_projects")
            .join("C3D_Volcano_Snow_Material_Complex.terrain")
    });
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(2048);
    let params = GaeaVolcanoSnowMaterialParams {
        volcano_scale: optional_f32_flag(cli, "volcano-scale")?.unwrap_or(1.28),
        volcano_height: optional_f32_flag(cli, "volcano-height")?.unwrap_or(1.18),
        volcano_mouth: optional_f32_flag(cli, "mouth")?.unwrap_or(0.23),
        volcano_bulk: optional_f32_flag(cli, "bulk")?.unwrap_or(-0.24),
        volcano_surface: cli.flag("surface").unwrap_or("Eroded").to_string(),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or(43851),
        snow_intensity: optional_f32_flag(cli, "snow-intensity")?.unwrap_or(0.82),
        snow_mass: optional_f32_flag(cli, "snow-mass")?.unwrap_or(16.0),
        snow_settle_thaw: optional_f32_flag(cli, "snow-settle-thaw")?.unwrap_or(0.22),
        snow_direction: cli.flag("snow-direction").unwrap_or("E").to_string(),
        rock_library: cli.flag("rock-library").unwrap_or("Sand").to_string(),
        rock_library_item: optional_i32_flag(cli, "rock-library-item")?.unwrap_or(240),
        snow_library: cli.flag("snow-library").unwrap_or("Blue").to_string(),
        snow_library_item: optional_i32_flag(cli, "snow-library-item")?.unwrap_or(104),
        tree_count: optional_i32_flag(cli, "tree-count")?.unwrap_or(180),
        tree_size: optional_f32_flag(cli, "tree-size")?.unwrap_or(0.085),
        tree_altitude_max: optional_f32_flag(cli, "tree-altitude-max")?.unwrap_or(0.36),
        tree_slope_max: optional_f32_flag(cli, "tree-slope-max")?.unwrap_or(24.0),
        tree_library_item: optional_i32_flag(cli, "tree-library-item")?.unwrap_or(315),
    };
    let open = cli.has("open");
    let command_preview = format!(
        "{} gaea-project --preset {preset} --template \"{}\" --output \"{}\" --resolution {resolution} --run{}",
        TOOL_COMMAND,
        template.display(),
        output.display(),
        if open { " --open" } else { "" }
    );
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gaea-project",
            "preset": preset,
            "template": template,
            "output": output,
            "resolution": resolution,
            "open": open,
            "volcano_params": params.to_json(),
            "graph_plan": gaea_volcano_snow_graph_plan(),
            "command_preview": command_preview,
            "truth_rule": "This command creates a native Gaea .terrain project for harness-driven node exploration; it does not claim Cunning3D parity."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let mut project: Value = read_json(&template)?;
    apply_volcano_snow_material_preset(&mut project, &params, resolution)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&output, &project)?;
    let _: Value = read_json(&output)?;
    let open_status = if open {
        Some(open_gaea_project(&output))
    } else {
        None
    };
    let payload = json!({
        "mode": "executed",
        "command": "gaea-project",
        "preset": preset,
        "template": template,
        "output": output,
        "resolution": resolution,
        "open": open,
        "open_status": open_status,
        "volcano_params": params.to_json(),
        "graph_plan": gaea_volcano_snow_graph_plan(),
        "selected_node": 890,
        "terminal_height_node": 885,
        "terminal_material_node": 890,
        "truth_rule": "Generated Gaea projects are harness fixtures for driving Gaea itself. Bridge/raw-buffer parity remains the migration oracle when this graph is ported."
    });
    let summary_dir = ctx
        .artifact_root
        .join("gaea_projects")
        .join(format!("summary_{}", unix_stamp_millis()));
    fs::create_dir_all(&summary_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", summary_dir.display()))?;
    write_pretty_json(&summary_dir.join("gaea_project_summary.json"), &payload)?;
    print_value(cli.json(), &payload);
    Ok(())
}

#[derive(Debug)]
struct GaeaVolcanoSnowMaterialParams {
    volcano_scale: f32,
    volcano_height: f32,
    volcano_mouth: f32,
    volcano_bulk: f32,
    volcano_surface: String,
    seed: i32,
    snow_intensity: f32,
    snow_mass: f32,
    snow_settle_thaw: f32,
    snow_direction: String,
    rock_library: String,
    rock_library_item: i32,
    snow_library: String,
    snow_library_item: i32,
    tree_count: i32,
    tree_size: f32,
    tree_altitude_max: f32,
    tree_slope_max: f32,
    tree_library_item: i32,
}

impl GaeaVolcanoSnowMaterialParams {
    fn to_json(&self) -> Value {
        json!({
            "volcano": {
                "Scale": self.volcano_scale,
                "Height": self.volcano_height,
                "Mouth": self.volcano_mouth,
                "Bulk": self.volcano_bulk,
                "Surface": self.volcano_surface,
                "Seed": self.seed,
                "X": 0.5,
                "Y": 0.48
            },
            "snowfield": {
                "Intensity": self.snow_intensity,
                "AdheredSnowMass": self.snow_mass,
                "SettleThaw": self.snow_settle_thaw,
                "Direction": self.snow_direction,
                "Seed": self.seed + 17
            },
            "material": {
                "rock_library": self.rock_library,
                "rock_library_item": self.rock_library_item,
                "snow_library": self.snow_library,
                "snow_library_item": self.snow_library_item
            },
            "trees": {
                "TreeCount": self.tree_count,
                "TreeSize": self.tree_size,
                "Altitude": {"X": 0.0, "Y": self.tree_altitude_max},
                "Slope": {"X": 0.0, "Y": self.tree_slope_max},
                "Inhibition": "Snowfield.Snow",
                "green_library": "Green",
                "green_library_item": self.tree_library_item
            }
        })
    }
}

fn apply_volcano_snow_material_preset(
    project: &mut Value,
    params: &GaeaVolcanoSnowMaterialParams,
    resolution: u32,
) -> Result<(), String> {
    let asset = gaea_primary_asset_object_mut(project)?;
    {
        let terrain = asset
            .get_mut("Terrain")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea project asset does not contain a Terrain object.".to_string())?;
        set_object_string_field(terrain, "Id", "c3d0a8a2-6d3b-4db9-9d76-a18fb43c0f21");
        if let Some(metadata) = terrain.get_mut("Metadata").and_then(Value::as_object_mut) {
            set_object_string_field(metadata, "Name", "C3D Complex Volcano Snow Material");
            set_object_string_field(
                metadata,
                "Description",
                "Generated by C3D harness: Volcano source, thermal shaping, erosion, rock strata, snowfield, rock/snow SatMap blend, and ColorErosion material.",
            );
            set_object_string_field(metadata, "ModifiedVersion", "2.2.0.0");
        }
        let nodes = terrain
            .get_mut("Nodes")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea terrain has no Nodes object.".to_string())?;
        configure_source_volcano(nodes, params)?;
        tune_existing_snow_peak_chain(nodes, params)?;
        insert_snowy_volcano_material_nodes(nodes, params);
    }
    if let Some(state) = asset.get_mut("State").and_then(Value::as_object_mut) {
        state.insert("SelectedNode".to_string(), json!(890));
        if let Some(viewport) = state.get_mut("Viewport").and_then(Value::as_object_mut) {
            set_object_string_field(viewport, "RenderMode", "Realistic");
            viewport.insert("AmbientOcclusion".to_string(), json!(true));
            viewport.insert("Shadows".to_string(), json!(true));
        }
    }
    if let Some(build) = asset
        .get_mut("BuildDefinition")
        .and_then(Value::as_object_mut)
    {
        build.insert("Resolution".to_string(), json!(resolution));
        build.insert("BakeResolution".to_string(), json!(resolution));
        build.insert("BucketResolution".to_string(), json!(resolution));
        build.insert(
            "TileResolution".to_string(),
            json!((resolution / 2).max(256)),
        );
    }
    Ok(())
}

fn configure_source_volcano(
    nodes: &mut serde_json::Map<String, Value>,
    params: &GaeaVolcanoSnowMaterialParams,
) -> Result<(), String> {
    let source = nodes
        .get_mut("151")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Template node 151 was not found.".to_string())?;
    set_object_string_field(
        source,
        "$type",
        "QuadSpinner.Gaea.Nodes.Volcano, Gaea.Nodes",
    );
    set_object_string_field(source, "Name", "Volcano");
    source.remove("Style");
    source.insert("Scale".to_string(), json!(params.volcano_scale));
    source.insert("Height".to_string(), json!(params.volcano_height));
    source.insert("Mouth".to_string(), json!(params.volcano_mouth));
    source.insert("Bulk".to_string(), json!(params.volcano_bulk));
    source.insert("Surface".to_string(), json!(params.volcano_surface));
    source.insert("X".to_string(), json!(0.5));
    source.insert("Y".to_string(), json!(0.48));
    source.insert("Seed".to_string(), json!(params.seed));
    Ok(())
}

fn tune_existing_snow_peak_chain(
    nodes: &mut serde_json::Map<String, Value>,
    params: &GaeaVolcanoSnowMaterialParams,
) -> Result<(), String> {
    if let Some(erosion) = nodes.get_mut("970").and_then(Value::as_object_mut) {
        erosion.insert("Duration".to_string(), json!(18.0));
        erosion.insert("Downcutting".to_string(), json!(0.18));
        erosion.insert("ErosionScale".to_string(), json!(118.0));
        erosion.insert("DirectionalPrecipitation".to_string(), json!(true));
        erosion.insert("Direction".to_string(), json!(125));
        erosion.insert("RainShadow".to_string(), json!(0.08));
        erosion.insert("Seed".to_string(), json!(params.seed + 3));
    }
    if let Some(outcrops) = nodes.get_mut("562").and_then(Value::as_object_mut) {
        outcrops.insert("Variations".to_string(), json!(6));
        outcrops.insert("Strata".to_string(), json!(0.56));
        outcrops.insert("Density".to_string(), json!(0.74));
        outcrops.insert("Shape".to_string(), json!(0.58));
        outcrops.insert("Seed".to_string(), json!(params.seed + 7));
    }
    if let Some(sandstone) = nodes.get_mut("558").and_then(Value::as_object_mut) {
        sandstone.insert("Passes".to_string(), json!(4));
        sandstone.insert("Spacing".to_string(), json!(0.29));
        sandstone.insert("Convexity".to_string(), json!(-0.18));
        sandstone.insert("Tilt".to_string(), json!(0.42));
        sandstone.insert("Chaos".to_string(), json!(0.46));
        sandstone.insert("Seed".to_string(), json!(params.seed + 11));
    }
    if let Some(snowfield) = nodes.get_mut("295").and_then(Value::as_object_mut) {
        snowfield.insert("Cascades".to_string(), json!(36));
        snowfield.insert("Duration".to_string(), json!(0.36));
        snowfield.insert("Intensity".to_string(), json!(params.snow_intensity));
        snowfield.insert("SettleThaw".to_string(), json!(params.snow_settle_thaw));
        snowfield.insert("AdheredSnowMass".to_string(), json!(params.snow_mass));
        snowfield.insert("Direction".to_string(), json!(params.snow_direction));
        snowfield.insert("Seed".to_string(), json!(params.seed + 17));
    }
    Ok(())
}

fn insert_snowy_volcano_material_nodes(
    nodes: &mut serde_json::Map<String, Value>,
    params: &GaeaVolcanoSnowMaterialParams,
) {
    nodes.insert(
        "880".to_string(),
        json!({
            "$id": "9000",
            "$type": "QuadSpinner.Gaea.Nodes.TextureBase, Gaea.Nodes",
            "Slope": 0.34,
            "Scale": 0.62,
            "Soil": 0.18,
            "Patches": 0.36,
            "Chaos": 0.88,
            "Seed": params.seed + 23,
            "Id": 880,
            "Version": 2,
            "Name": "VolcanicTextureBase",
            "Position": {"$id": "9001", "X": 28460.0, "Y": 26220.0},
            "Ports": {"$id": "9002", "$values": [
                gaea_port("9003", "In", "PrimaryIn, Required", "9000", Some(gaea_record("9004", 295, 880, "Out", "In"))),
                gaea_port("9005", "Out", "PrimaryOut", "9000", None),
                gaea_port("9006", "Guide", "In", "9000", None)
            ]},
            "Modifiers": {"$id": "9007", "$values": []}
        }),
    );
    nodes.insert(
        "881".to_string(),
        json!({
            "$id": "9010",
            "$type": "QuadSpinner.Gaea.Nodes.SatMap, Gaea.Nodes",
            "Library": params.rock_library,
            "LibraryItem": params.rock_library_item,
            "Range": {"$id": "9011", "X": 0.08, "Y": 1.0},
            "Bias": -0.08,
            "Enhance": "Equalize",
            "Saturation": -0.12,
            "Lightness": -0.06,
            "Id": 881,
            "Name": "VolcanicRockSatMap",
            "Position": {"$id": "9012", "X": 28760.0, "Y": 26220.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9013", "$values": [
                gaea_port("9014", "In", "PrimaryIn, Required", "9010", Some(gaea_record("9015", 880, 881, "Out", "In"))),
                gaea_port("9016", "Out", "PrimaryOut", "9010", None)
            ]},
            "Modifiers": {"$id": "9017", "$values": []}
        }),
    );
    nodes.insert(
        "882".to_string(),
        json!({
            "$id": "9020",
            "$type": "QuadSpinner.Gaea.Nodes.SatMap, Gaea.Nodes",
            "Library": params.snow_library,
            "LibraryItem": params.snow_library_item,
            "Range": {"$id": "9021", "X": 0.58, "Y": 1.0},
            "Bias": 0.12,
            "Enhance": "None",
            "Rough": "Med",
            "Saturation": -0.18,
            "Lightness": 0.42,
            "Id": 882,
            "Name": "SnowSatMap",
            "NodeSize": "Small",
            "Position": {"$id": "9022", "X": 28760.0, "Y": 26400.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9023", "$values": [
                gaea_port("9024", "In", "PrimaryIn, Required", "9020", Some(gaea_record("9025", 295, 882, "Snow", "In"))),
                gaea_port("9026", "Out", "PrimaryOut", "9020", None)
            ]},
            "Modifiers": {"$id": "9027", "$values": []}
        }),
    );
    nodes.insert(
        "883".to_string(),
        json!({
            "$id": "9030",
            "$type": "QuadSpinner.Gaea.Nodes.Combine, Gaea.Nodes",
            "PortCount": 2,
            "Ratio": 1.0,
            "Id": 883,
            "Name": "RockSnowMaterialMix",
            "NodeSize": "Small",
            "Position": {"$id": "9031", "X": 29080.0, "Y": 26310.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9032", "$values": [
                gaea_port("9033", "In", "PrimaryIn, Required", "9030", Some(gaea_record("9034", 881, 883, "Out", "In"))),
                gaea_port("9035", "Out", "PrimaryOut", "9030", None),
                gaea_port("9036", "Input2", "In", "9030", Some(gaea_record("9037", 882, 883, "Out", "Input2"))),
                gaea_port("9038", "Mask", "In", "9030", Some(gaea_record("9039", 295, 883, "Snow", "Mask")))
            ]},
            "Modifiers": {"$id": "9040", "$values": []}
        }),
    );
    nodes.insert(
        "884".to_string(),
        json!({
            "$id": "9050",
            "$type": "QuadSpinner.Gaea.Nodes.ColorErosion, Gaea.Nodes",
            "TransportDistance": 1.35,
            "SedimentDensity": 0.72,
            "Blend": 0.82,
            "ColorHold": 0.76,
            "LaminarFlow": true,
            "Diffusion": 0.28,
            "Seed": params.seed + 31,
            "Id": 884,
            "Name": "SnowyVolcanoColorErosion",
            "Position": {"$id": "9051", "X": 29400.0, "Y": 26310.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9052", "$values": [
                gaea_port("9053", "In", "PrimaryIn, Required", "9050", Some(gaea_record("9054", 883, 884, "Out", "In"))),
                gaea_port("9055", "Out", "PrimaryOut", "9050", None),
                gaea_port("9056", "Height", "In", "9050", Some(gaea_record("9057", 295, 884, "Out", "Height"))),
                gaea_port("9058", "Precipitation", "In", "9050", None)
            ]},
            "Modifiers": {"$id": "9059", "$values": []}
        }),
    );
    nodes.insert(
        "885".to_string(),
        json!({
            "$id": "9060",
            "$type": "QuadSpinner.Gaea.Nodes.Trees, Gaea.Nodes",
            "TreeCount": params.tree_count,
            "TreeSize": params.tree_size,
            "TrimUnder": 0.02,
            "Seed": params.seed + 37,
            "Health": 0.86,
            "Patches": 0.16,
            "Spread": 0.22,
            "Slope": {"$id": "9061", "X": 0.0, "Y": params.tree_slope_max},
            "SlopeFalloff": 18.0,
            "Altitude": {"$id": "9062", "X": 0.0, "Y": params.tree_altitude_max},
            "AltitudeFalloff": 0.34,
            "Peaks": 0.08,
            "DeadFlow": 0.72,
            "ConsolidateFlows": 0.46,
            "Bias": 0.58,
            "Snowline": 0.24,
            "Chaos": 0.18,
            "Trim": 0.04,
            "Id": 885,
            "Version": 2,
            "Name": "FootForestTrees",
            "Position": {"$id": "9063", "X": 28480.0, "Y": 26620.0},
            "Ports": {"$id": "9064", "$values": [
                gaea_port("9065", "In", "PrimaryIn, Required", "9060", Some(gaea_record("9066", 295, 885, "Out", "In"))),
                gaea_port("9067", "Out", "PrimaryOut", "9060", None),
                gaea_port("9068", "Inhibition", "In", "9060", Some(gaea_record("9069", 295, 885, "Snow", "Inhibition"))),
                gaea_port("9070", "DeadZones", "Out", "9060", None),
                gaea_port("9071", "FreshWater", "Out", "9060", None),
                gaea_port("9072", "Trees", "Out", "9060", None)
            ]},
            "Modifiers": {"$id": "9073", "$values": []}
        }),
    );
    nodes.insert(
        "886".to_string(),
        json!({
            "$id": "9080",
            "$type": "QuadSpinner.Gaea.Nodes.Adjust, Gaea.Nodes",
            "Equalize": true,
            "Id": 886,
            "Name": "FootForestMask",
            "NodeSize": "Small",
            "Position": {"$id": "9081", "X": 28800.0, "Y": 26620.0},
            "RenderIntentOverride": "Mask",
            "Ports": {"$id": "9082", "$values": [
                gaea_port("9083", "In", "PrimaryIn, Required", "9080", Some(gaea_record("9084", 885, 886, "Trees", "In"))),
                gaea_port("9085", "Out", "PrimaryOut", "9080", None)
            ]},
            "Modifiers": {"$id": "9086", "$values": []}
        }),
    );
    nodes.insert(
        "887".to_string(),
        json!({
            "$id": "9090",
            "$type": "QuadSpinner.Gaea.Nodes.Noise, Gaea.Nodes",
            "Scale": 0.36,
            "Octaves": 7,
            "Seed": params.seed + 41,
            "Id": 887,
            "Name": "ForestColorNoise",
            "NodeSize": "Small",
            "Position": {"$id": "9091", "X": 28640.0, "Y": 26810.0},
            "Ports": {"$id": "9092", "$values": [
                gaea_port("9093", "In", "PrimaryIn", "9090", None),
                gaea_port("9094", "Out", "PrimaryOut", "9090", None)
            ]},
            "Modifiers": {"$id": "9095", "$values": []}
        }),
    );
    nodes.insert(
        "888".to_string(),
        json!({
            "$id": "9100",
            "$type": "QuadSpinner.Gaea.Nodes.SatMap, Gaea.Nodes",
            "Library": "Green",
            "LibraryItem": params.tree_library_item,
            "Rough": "High",
            "Bias": -0.04,
            "Saturation": 0.16,
            "Lightness": -0.08,
            "Id": 888,
            "Name": "ForestGreenSatMap",
            "NodeSize": "Small",
            "Position": {"$id": "9101", "X": 28920.0, "Y": 26810.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9102", "$values": [
                gaea_port("9103", "In", "PrimaryIn, Required", "9100", Some(gaea_record("9104", 887, 888, "Out", "In"))),
                gaea_port("9105", "Out", "PrimaryOut", "9100", None)
            ]},
            "Modifiers": {"$id": "9106", "$values": []}
        }),
    );
    nodes.insert(
        "889".to_string(),
        json!({
            "$id": "9110",
            "$type": "QuadSpinner.Gaea.Nodes.Weathering, Gaea.Nodes",
            "Scale": 0.052,
            "WashedOut": true,
            "Dirt": 0.31,
            "Darker": true,
            "Id": 889,
            "Name": "VolcanicAshWeathering",
            "Position": {"$id": "9111", "X": 29700.0, "Y": 26310.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9112", "$values": [
                gaea_port("9113", "In", "PrimaryIn, Required", "9110", Some(gaea_record("9114", 884, 889, "Out", "In"))),
                gaea_port("9115", "Out", "PrimaryOut", "9110", None),
                gaea_port("9116", "Height", "In", "9110", Some(gaea_record("9117", 885, 889, "Out", "Height")))
            ]},
            "Modifiers": {"$id": "9118", "$values": []}
        }),
    );
    nodes.insert(
        "890".to_string(),
        json!({
            "$id": "9120",
            "$type": "QuadSpinner.Gaea.Nodes.Combine, Gaea.Nodes",
            "PortCount": 2,
            "Ratio": 1.0,
            "Id": 890,
            "Name": "FinalSnowyVolcanoForestMaterial",
            "NodeSize": "Standard",
            "Position": {"$id": "9121", "X": 30040.0, "Y": 26430.0},
            "RenderIntentOverride": "Color",
            "Ports": {"$id": "9122", "$values": [
                gaea_port("9123", "In", "PrimaryIn, Required", "9120", Some(gaea_record("9124", 889, 890, "Out", "In"))),
                gaea_port("9125", "Out", "PrimaryOut", "9120", None),
                gaea_port("9126", "Input2", "In", "9120", Some(gaea_record("9127", 888, 890, "Out", "Input2"))),
                gaea_port("9128", "Mask", "In", "9120", Some(gaea_record("9129", 886, 890, "Out", "Mask")))
            ]},
            "Modifiers": {"$id": "9130", "$values": []}
        }),
    );
}

fn gaea_primary_asset_object_mut(
    project: &mut Value,
) -> Result<&mut serde_json::Map<String, Value>, String> {
    project
        .get_mut("Assets")
        .and_then(|value| value.get_mut("$values"))
        .and_then(Value::as_array_mut)
        .and_then(|assets| assets.get_mut(0))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "Gaea project does not contain Assets.$values[0].".to_string())
}

fn gaea_record(id: &str, from: i32, to: i32, from_port: &str, to_port: &str) -> Value {
    json!({
        "$id": id,
        "From": from,
        "To": to,
        "FromPort": from_port,
        "ToPort": to_port,
        "IsValid": true
    })
}

fn gaea_port(
    id: &str,
    name: &str,
    type_name: &str,
    parent_ref: &str,
    record: Option<Value>,
) -> Value {
    let mut port = serde_json::Map::new();
    port.insert("$id".to_string(), json!(id));
    port.insert("Name".to_string(), json!(name));
    port.insert("Type".to_string(), json!(type_name));
    if let Some(record) = record {
        port.insert("Record".to_string(), record);
    }
    port.insert("IsExporting".to_string(), json!(true));
    port.insert("Parent".to_string(), json!({ "$ref": parent_ref }));
    Value::Object(port)
}

fn set_object_string_field(object: &mut serde_json::Map<String, Value>, key: &str, value: &str) {
    object.insert(key.to_string(), json!(value));
}

fn gaea_volcano_snow_graph_plan() -> Value {
    json!([
        "151 Volcano -> 970 Erosion2 -> 789 ThermalShaper -> 562 Outcrops -> 558 Sandstone -> 295 Snowfield",
        "295 Snowfield.Out -> 880 TextureBase -> 881 VolcanicRockSatMap",
        "295 Snowfield.Snow -> 882 SnowSatMap",
        "881 rock color + 882 snow color mixed by 295 Snowfield.Snow -> 883 Combine",
        "883 material color + 295 Snowfield.Out height -> 884 ColorErosion",
        "295 Snowfield.Out + 295 Snowfield.Snow inhibition -> 885 FootForestTrees",
        "885 Trees -> 886 FootForestMask; 887 Noise -> 888 ForestGreenSatMap",
        "884 ColorErosion + 885 height -> 889 Weathering; 889 material + 888 forest color masked by 886 -> 890 final material"
    ])
}

fn open_gaea_project(path: &Path) -> Value {
    let mut command = Command::new("cmd");
    command.args(["/C", "start", "", &path.display().to_string()]);
    match command.spawn() {
        Ok(child) => json!({
            "spawned": true,
            "pid": child.id(),
            "command_preview": command_preview(&command)
        }),
        Err(error) => json!({
            "spawned": false,
            "error": error.to_string(),
            "command_preview": command_preview(&command)
        }),
    }
}
