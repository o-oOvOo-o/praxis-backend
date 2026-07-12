
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

fn cmd_perf_migrate(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "perf-migrate");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?.unwrap_or_else(|| {
        if seconds.is_some() {
            1_000_000
        } else {
            4
        }
    });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let candidates = perf_candidate_backends(cli)?;
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    let target_speedup = optional_f64_flag(cli, "target-speedup")?
        .or(optional_f64_flag(cli, "min-gaea-app-speedup")?)
        .unwrap_or(5.0);
    let gaea_app_baseline_ms = optional_f64_flag(cli, "gaea-app-baseline-ms")?;
    let native_preflight = !cli.has("skip-native-preflight") && backend_name_is_bridge(rhs_backend);
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(1.0e-4);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(2.0e-4);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(2.0e-3);
    let style_cycle = style_choices(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let mut commands = Vec::new();
        let mut first_preview_params = None;
        for index in 0..preview_count {
            let params =
                mountain_candidate_sweep_params(cli, &mut preview_rng, index, &style_cycle)?;
            if first_preview_params.is_none() {
                first_preview_params = Some(params.to_json());
            }
            commands.push(json!({
                "index": params.index,
                "style_family": mountain_style_family(&params.style),
                "params": params.to_json(),
                "preflight": native_preflight.then(|| {
                    command_preview(&mountain_native_bridge_preflight_command(ctx, cli, &params))
                }),
                "candidates": candidates.iter().map(|candidate| {
                    json!({
                        "backend": candidate,
                        "backend_role": backend_role_view(candidate, cli),
                        "command": command_preview(&mountain_gpu_sweep_command(
                            ctx,
                            cli,
                            &params,
                            candidate,
                            rhs_backend,
                            mean_abs_norm_limit,
                            rmse_norm_limit,
                            max_abs_norm_limit,
                        )),
                    })
                }).collect::<Vec<_>>(),
            }));
        }
        let next_min_focused_cargo_run = candidates.first().map(|candidate| {
            mountain_backend_compare_cargo_command_from_params(
                &ctx.cunning_core_manifest,
                candidate,
                rhs_backend,
                first_preview_params.as_ref(),
                cli,
                &[],
            )
        });
        let next_focused_command = candidates.first().map(|candidate| {
            gpu_sweep_tool_command_from_params(
                candidate,
                rhs_backend,
                cli,
                first_preview_params.as_ref(),
                &["--require-gpu-active"],
            )
        });
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "perf-migrate",
                "node": "Mountain",
                "candidate_backends": candidates.clone(),
                "rhs_backend": rhs_backend,
                "execution_roles": perf_execution_roles(&candidates, rhs_backend, cli),
                "native_preflight": native_preflight,
                "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
                "target_speedup_vs_gaea_app": target_speedup,
                "gaea_app_baseline_ms": gaea_app_baseline_ms,
                "speed_gate_active": gaea_app_baseline_ms.is_some(),
                "aggregation_schema": {
                    "best_exact_candidate": "Fastest Bridge-exact candidate across executed artifacts.",
                    "fastest_non_exact_candidate": "Fastest candidate that did not prove exact raw-buffer parity.",
                    "gpu_activity_status": "Aggregated GPU active/readback/submit state by backend and across the run.",
                    "engineering_report": "Promotion-oriented gate report with Bridge oracle status, Gaea app speed gate, first mismatch, and next commands.",
                    "next_focused_command": "Single tool rerun command for the first blocking or non-exact report.",
                    "next_min_focused_cargo_run": "Smallest direct cargo run for the same first focused repro."
                },
                "engineering_fields": [
                    "promotion_status",
                    "bridge_oracle_gate",
                    "gaea_app_speed_gate",
                    "first_mismatch",
                    "next_commands"
                ],
                "next_focused_command": next_focused_command,
                "next_min_focused_cargo_run": next_min_focused_cargo_run,
                "rng_seed": rng_seed,
                "requested_samples": requested_samples,
                "seconds": seconds,
                "commands": commands,
                "truth_rule": "Bridge raw buffers gate correctness first; Gaea desktop app baseline gates the 4-5x performance target. CPU, GPU, and hybrid candidates are all allowed."
            }),
        );
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("perf_migrate").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut samples = Vec::new();
    let mut candidate_run_count = 0usize;
    let mut candidate_correct_count = 0usize;
    let mut candidate_speed_pass_count = 0usize;
    let mut sample_accept_count = 0usize;
    let mut oracle_gap_count = 0usize;
    let mut first_blocker = None;
    let mut best_overall = None;
    let mut best_overall_speedup = f64::NEG_INFINITY;
    let mut best_exact_candidate = None;
    let mut best_exact_rank = f64::NEG_INFINITY;
    let mut fastest_non_exact_candidate = None;
    let mut fastest_non_exact_rank = f64::NEG_INFINITY;
    let mut first_failed_report = None;
    let mut candidate_stats: BTreeMap<String, PerfBackendStats> = BTreeMap::new();
    let mut gpu_activity_summary = GpuActivityAccumulator::default();
    let mut gpu_profile_summary = GpuProfileAccumulator::default();
    let mut cpu_cache_profile_summary = CpuCacheProfileAccumulator::default();

    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_candidate_sweep_params(cli, &mut rng, index, &style_cycle)?;
        let mut preflight_summary = None;
        if native_preflight {
            let mut command = mountain_native_bridge_preflight_command(ctx, cli, &params);
            apply_fresh_bridge_cache_env(
                &mut command,
                cli,
                &run_dir,
                &format!("{:04}_preflight", params.index),
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path = run_dir.join(format!("{:04}_preflight_stdout.json", params.index));
            let stderr_path = run_dir.join(format!("{:04}_preflight_stderr.txt", params.index));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            preflight_summary = Some(json!({
                "command": preview,
                "status": output.status_code,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "summary": parsed.as_ref().and_then(summary_view),
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "cpu_cache_profile": parsed.as_ref().and_then(backend_compare_cpu_cache_profile_view),
            }));
            if !(exact && output.status_code == 0) {
                oracle_gap_count += 1;
                let blocker = json!({
                    "kind": "native_bridge_preflight_gap",
                    "index": params.index,
                    "params": params.to_json(),
                    "preflight": preflight_summary,
                });
                if first_blocker.is_none() {
                    first_blocker = Some(blocker.clone());
                }
                samples.push(json!({
                    "index": params.index,
                    "status_kind": "oracle_contract_gap",
                    "accepted": false,
                    "params": params.to_json(),
                    "preflight": preflight_summary,
                    "candidates": [],
                }));
                if !cli.has("keep-going") {
                    break;
                }
                continue;
            }
        }

        let mut candidate_results = Vec::new();
        let mut sample_best = None;
        let mut sample_best_speedup = f64::NEG_INFINITY;
        let mut sample_accepted = false;
        let mut sample_correct = false;
        let mut sample_cpu_baseline_elapsed_ms = None;
        for candidate in &candidates {
            candidate_run_count += 1;
            let mut command = mountain_gpu_sweep_command(
                ctx,
                cli,
                &params,
                candidate,
                rhs_backend,
                mean_abs_norm_limit,
                rmse_norm_limit,
                max_abs_norm_limit,
            );
            apply_fresh_bridge_cache_env(
                &mut command,
                cli,
                &run_dir,
                &format!("{:04}_{candidate}", params.index),
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path =
                run_dir.join(format!("{:04}_{}_stdout.json", params.index, candidate));
            let stderr_path = run_dir.join(format!("{:04}_{}_stderr.txt", params.index, candidate));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let compare_passed = output.status_code == 0
                && parsed.as_ref().map(backend_compare_passed).unwrap_or(false);
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            if compare_passed {
                candidate_correct_count += 1;
                sample_correct = true;
            }
            let candidate_elapsed_ms =
                local_candidate_elapsed_ms(parsed.as_ref(), candidate, rhs_backend);
            let speedup =
                gaea_app_baseline_ms
                    .zip(candidate_elapsed_ms)
                    .and_then(|(baseline, elapsed)| {
                        (baseline > 0.0 && elapsed > 0.0).then_some(baseline / elapsed)
                    });
            let speed_passed = speedup.map(|value| value >= target_speedup);
            if compare_passed && speed_passed.unwrap_or(false) {
                candidate_speed_pass_count += 1;
                sample_accepted = true;
            }
            let profile = parsed.as_ref().and_then(backend_compare_total_gpu_profile);
            let activity = profile
                .map(gpu_activity_view)
                .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
            let diagnosis = perf_candidate_diagnosis(
                candidate,
                &params,
                rhs_backend,
                parsed.as_ref(),
                output.status_code,
                compare_passed,
                exact,
                candidate_elapsed_ms,
                speedup,
                speed_passed,
                gaea_app_baseline_ms,
                target_speedup,
                &activity,
                cli,
                sample_cpu_baseline_elapsed_ms,
            );
            gpu_activity_summary.push(&activity);
            if let Some(parsed) = parsed.as_ref() {
                gpu_profile_summary.push_from_compare(parsed);
                cpu_cache_profile_summary.push_from_compare(parsed);
            }
            let candidate_report = json!({
                "backend": candidate,
                "backend_role": backend_role_view(candidate, cli),
                "command": preview,
                "status": output.status_code,
                "compare_passed": compare_passed,
                "exact": exact,
                "candidate_elapsed_ms": candidate_elapsed_ms,
                "gaea_app_speedup": speedup,
                "speed_passed": speed_passed,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "cpu_cache_profile": parsed.as_ref().and_then(backend_compare_cpu_cache_profile_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "gpu_activity": activity,
                "diagnosis": diagnosis,
                "summary": parsed.as_ref().and_then(summary_view),
            });
            let candidate_focus = perf_candidate_focus_view(
                candidate,
                &params,
                output.status_code,
                compare_passed,
                exact,
                candidate_elapsed_ms,
                speedup,
                speed_passed,
                &stdout_path,
                &stderr_path,
                &activity,
                &candidate_report["diagnosis"],
                parsed.as_ref().and_then(summary_view),
                cli,
            );
            candidate_stats.entry(candidate.clone()).or_default().push(
                output.status_code,
                parsed.as_ref(),
                compare_passed,
                exact,
                speed_passed,
                candidate_elapsed_ms,
                speedup,
                &activity,
                &candidate_report["diagnosis"],
                &candidate_focus,
            );
            if !backend_name_is_gpu_candidate(candidate) && candidate_elapsed_ms.is_some() {
                sample_cpu_baseline_elapsed_ms = candidate_elapsed_ms;
            }
            if let Some(rank) = perf_candidate_rank(candidate_elapsed_ms, speedup) {
                if exact && rank > best_exact_rank {
                    best_exact_rank = rank;
                    best_exact_candidate = Some(candidate_focus.clone());
                } else if !exact && rank > fastest_non_exact_rank {
                    fastest_non_exact_rank = rank;
                    fastest_non_exact_candidate = Some(candidate_focus.clone());
                }
            }
            if first_failed_report.is_none()
                && (output.status_code != 0
                    || !compare_passed
                    || !exact
                    || speed_passed == Some(false))
            {
                first_failed_report = Some(candidate_focus);
            }
            if compare_passed {
                let rank_speedup = speedup.unwrap_or_else(|| {
                    candidate_elapsed_ms
                        .filter(|elapsed| *elapsed > 0.0)
                        .map(|elapsed| 1.0 / elapsed)
                        .unwrap_or(f64::NEG_INFINITY)
                });
                if rank_speedup > sample_best_speedup {
                    sample_best_speedup = rank_speedup;
                    sample_best = Some(candidate_report.clone());
                }
                if rank_speedup > best_overall_speedup {
                    best_overall_speedup = rank_speedup;
                    best_overall = Some(json!({
                        "sample_index": params.index,
                        "params": params.to_json(),
                        "candidate": candidate_report,
                    }));
                }
            }
            candidate_results.push(candidate_report);
        }
        if sample_accepted {
            sample_accept_count += 1;
        } else if first_blocker.is_none() {
            first_blocker = Some(json!({
                "kind": if !sample_correct {
                    "no_candidate_met_bridge_correctness"
                } else if gaea_app_baseline_ms.is_some() {
                    "no_correct_candidate_met_speedup"
                } else {
                    "gaea_app_baseline_missing_for_speed_gate"
                },
                "index": params.index,
                "params": params.to_json(),
                "sample_best": sample_best,
            }));
        }
        samples.push(json!({
            "index": params.index,
            "status_kind": if sample_accepted {
                "accepted_speed_candidate"
            } else if !sample_correct {
                "blocked_no_correct_candidate"
            } else if gaea_app_baseline_ms.is_none() {
                "correctness_only_no_gaea_app_baseline"
            } else {
                "blocked_no_speed_candidate"
            },
            "accepted": sample_accepted,
            "params": params.to_json(),
            "preflight": preflight_summary,
            "sample_best": sample_best,
            "candidates": candidate_results,
        }));
        if cli.has("require-speedup") && !sample_accepted && !cli.has("keep-going") {
            break;
        }
    }

    let executed_samples = samples.len();
    let all_samples_have_speed_candidate =
        executed_samples > 0 && sample_accept_count == executed_samples;
    let speed_gate_active = gaea_app_baseline_ms.is_some();
    let candidate_backend_summary = candidate_stats
        .iter()
        .map(|(backend, stats)| (backend.clone(), stats.to_json()))
        .collect::<serde_json::Map<_, _>>();
    let next_focused_command =
        find_next_focused_command(first_failed_report.as_ref()).or_else(|| {
            perf_aggregation_next_command(
                &first_blocker,
                candidate_stats
                    .values()
                    .find_map(|stats| stats.first_blocker.as_ref()),
            )
        });
    let next_min_focused_cargo_run = perf_next_min_focused_cargo_run(
        &ctx.cunning_core_manifest,
        first_failed_report.as_ref(),
        &first_blocker,
        &candidates,
        rhs_backend,
        cli,
    );
    let engineering_report = perf_migrate_engineering_report(
        executed_samples,
        speed_gate_active,
        all_samples_have_speed_candidate,
        oracle_gap_count,
        candidate_run_count,
        candidate_correct_count,
        candidate_speed_pass_count,
        sample_accept_count,
        target_speedup,
        gaea_app_baseline_ms,
        best_exact_candidate.as_ref(),
        fastest_non_exact_candidate.as_ref(),
        first_failed_report.as_ref(),
        first_blocker.as_ref(),
        next_focused_command.as_deref(),
        next_min_focused_cargo_run.as_deref(),
    );
    let aggregation = json!({
        "best_exact_candidate": best_exact_candidate,
        "fastest_non_exact_candidate": fastest_non_exact_candidate,
        "first_failed_report": first_failed_report,
        "candidate_backend_summary": candidate_backend_summary,
        "gpu_activity_status": gpu_activity_summary.to_json(),
        "gpu_profile_counts": gpu_profile_summary.to_json(),
        "cpu_cache_profile_counts": cpu_cache_profile_summary.to_json(),
        "speedup_vs_gaea_app_baseline": {
            "baseline_ms": gaea_app_baseline_ms,
            "target_speedup": target_speedup,
            "gate_active": speed_gate_active,
            "candidate_speed_pass_count": candidate_speed_pass_count,
            "all_samples_have_speed_candidate": all_samples_have_speed_candidate,
        },
        "next_focused_command": next_focused_command.clone(),
        "next_min_focused_cargo_run": next_min_focused_cargo_run.clone(),
        "engineering_report": engineering_report.clone(),
    });
    let payload = json!({
        "mode": "executed",
        "command": "perf-migrate",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "candidate_backends": candidates.clone(),
        "rhs_backend": rhs_backend,
        "execution_roles": perf_execution_roles(&candidates, rhs_backend, cli),
        "native_preflight": native_preflight,
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": executed_samples,
        "elapsed_seconds": started_at.elapsed().as_secs_f64(),
        "target_speedup_vs_gaea_app": target_speedup,
        "gaea_app_baseline_ms": gaea_app_baseline_ms,
        "speed_gate_active": speed_gate_active,
        "candidate_run_count": candidate_run_count,
        "candidate_correct_count": candidate_correct_count,
        "candidate_speed_pass_count": candidate_speed_pass_count,
        "sample_accept_count": sample_accept_count,
        "oracle_gap_count": oracle_gap_count,
        "all_samples_have_speed_candidate": all_samples_have_speed_candidate,
        "best_overall": best_overall,
        "artifact_aggregation": aggregation,
        "engineering_report": engineering_report,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": next_min_focused_cargo_run,
        "first_blocker": first_blocker,
        "samples": samples,
        "truth_rule": "Bridge raw buffers gate correctness first; Gaea desktop app baseline gates the 4-5x performance target. CPU, GPU, and hybrid candidates are all allowed."
    });
    let summary_path = run_dir.join("perf_migrate_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-speedup") && (!speed_gate_active || !all_samples_have_speed_candidate) {
        return Err(format!(
            "Mountain performance migration did not meet the requested speed gate. See '{}'.",
            summary_path.display()
        ));
    }
    if cli.has("require-all-pass") && oracle_gap_count > 0 {
        return Err(format!(
            "Mountain performance migration found {oracle_gap_count} oracle gap sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-sweep");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?.unwrap_or_else(|| {
        if seconds.is_some() {
            1_000_000
        } else {
            8
        }
    });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let lhs_backend = cli.flag("lhs").unwrap_or("native_gpu_wave");
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    let native_preflight = !cli.has("skip-native-preflight") && backend_name_is_bridge(rhs_backend);
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(1.0e-4);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(2.0e-4);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(2.0e-3);
    let gpu_performance_limits = GpuPerformanceLimits::from_cli(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let params = (0..preview_count)
            .map(|index| mountain_sweep_params(cli, &mut preview_rng, index))
            .collect::<Result<Vec<_>, _>>()?;
        let commands = params
            .iter()
            .map(|params| {
                let preflight = native_preflight.then(|| {
                    command_preview(&mountain_native_bridge_preflight_command(ctx, cli, params))
                });
                let gpu = command_preview(&mountain_gpu_sweep_command(
                    ctx,
                    cli,
                    params,
                    lhs_backend,
                    rhs_backend,
                    mean_abs_norm_limit,
                    rmse_norm_limit,
                    max_abs_norm_limit,
                ));
                json!({
                    "index": params.index,
                    "preflight": preflight,
                    "gpu": gpu,
                    "lhs_role": backend_role_view(lhs_backend, cli),
                    "rhs_role": backend_role_view(rhs_backend, cli),
                })
            })
            .collect::<Vec<_>>();
        let next_min_focused_cargo_run = params.first().map(|params| {
            mountain_backend_compare_cargo_command_from_params(
                &ctx.cunning_core_manifest,
                lhs_backend,
                rhs_backend,
                Some(&params.to_json()),
                cli,
                &[],
            )
        });
        let next_focused_command = params.first().map(|params| {
            let params_json = params.to_json();
            gpu_sweep_tool_command_from_params(
                lhs_backend,
                rhs_backend,
                cli,
                Some(&params_json),
                &["--require-gpu-active"],
            )
        });
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-sweep",
            "node": "Mountain",
            "lhs_backend": lhs_backend,
            "rhs_backend": rhs_backend,
            "execution_roles": gpu_sweep_execution_roles(lhs_backend, rhs_backend, cli),
            "native_preflight": native_preflight,
            "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
            "require_gpu_active": cli.has("require-gpu-active"),
            "fresh_bridge_cache": cli.has("fresh-bridge-cache"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "gpu_performance_limits": gpu_performance_limits.to_json(),
            "performance_policy": {
                "correctness_oracle": "GaeaBridge raw buffers",
                "speed_baseline": "Measured Gaea desktop app cook time",
                "bridge_elapsed": "diagnostic_only"
            },
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "engineering_fields": [
                "promotion_status",
                "bridge_oracle_gate",
                "gaea_app_speed_gate",
                "first_mismatch",
                "next_commands"
            ],
            "next_focused_command": next_focused_command,
            "next_min_focused_cargo_run": next_min_focused_cargo_run,
            "tolerance": {
                "mean_abs_norm_limit": mean_abs_norm_limit,
                "rmse_norm_limit": rmse_norm_limit,
                "max_abs_norm_limit": max_abs_norm_limit,
                "require_exact": cli.has("require-exact")
            },
            "commands": commands,
            "note": "Pass --run to execute Bridge-oracle GPU migration compares. Use --gaea-app-baseline-ms with --min-gaea-app-speedup for real Gaea app performance gating."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_sweep").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut samples = Vec::new();
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let mut gpu_compare_failure_count = 0usize;
    let mut performance_gate_failure_count = 0usize;
    let mut oracle_gap_count = 0usize;
    let mut first_failure = None;
    let mut first_performance_gate_failure = None;
    let mut first_oracle_gap = None;
    let mut gpu_timing = TimingAccumulator::default();
    let mut preflight_timing = TimingAccumulator::default();
    let mut gpu_profile = GpuProfileAccumulator::default();
    let mut preflight_gpu_profile = GpuProfileAccumulator::default();
    let mut gpu_activity = GpuActivityAccumulator::default();
    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_sweep_params(cli, &mut rng, index)?;
        let mut preflight_summary = None;
        if native_preflight {
            let mut command = mountain_native_bridge_preflight_command(ctx, cli, &params);
            apply_fresh_bridge_cache_env(
                &mut command,
                cli,
                &run_dir,
                &format!("{:04}_preflight", params.index),
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path = run_dir.join(format!("{:04}_preflight_stdout.json", params.index));
            let stderr_path = run_dir.join(format!("{:04}_preflight_stderr.txt", params.index));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            if let Some(parsed) = parsed.as_ref() {
                preflight_timing.push_from_compare(parsed);
                preflight_gpu_profile.push_from_compare(parsed);
            }
            let preflight = json!({
                "command": preview,
                "status": output.status_code,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "summary": parsed.as_ref().and_then(summary_view),
            });
            if !(exact && output.status_code == 0) {
                oracle_gap_count += 1;
                if first_oracle_gap.is_none() {
                    first_oracle_gap = Some(json!({
                        "index": params.index,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "summary": parsed.as_ref().and_then(summary_view),
                    }));
                }
                samples.push(json!({
                    "index": params.index,
                    "status_kind": "oracle_contract_gap",
                    "passed": false,
                    "params": params.to_json(),
                    "preflight": preflight,
                    "gpu": null,
                }));
                if !cli.has("keep-going") {
                    break;
                }
                continue;
            }
            preflight_summary = Some(preflight);
        }
        let mut command = mountain_gpu_sweep_command(
            ctx,
            cli,
            &params,
            lhs_backend,
            rhs_backend,
            mean_abs_norm_limit,
            rmse_norm_limit,
            max_abs_norm_limit,
        );
        apply_fresh_bridge_cache_env(
            &mut command,
            cli,
            &run_dir,
            &format!("{:04}_gpu", params.index),
        );
        let preview = command_preview(&command);
        let output = run_capture_allow_failure(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!("{:04}_stdout.json", params.index));
        let stderr_path = run_dir.join(format!("{:04}_stderr.txt", params.index));
        write_text(&stdout_path, &stdout_text)?;
        write_text(&stderr_path, &output.stderr)?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let passed = parsed.as_ref().map(backend_compare_passed).unwrap_or(false);
        let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
        if let Some(parsed) = parsed.as_ref() {
            gpu_timing.push_from_compare(parsed);
            gpu_profile.push_from_compare(parsed);
        }
        let performance_gate = gpu_performance_gate_view(
            &gpu_performance_limits,
            parsed.as_ref().and_then(backend_compare_total_gpu_profile),
            cli.has("gpu-exact-barrier"),
        );
        let activity = parsed
            .as_ref()
            .and_then(backend_compare_total_gpu_profile)
            .map(gpu_activity_view)
            .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
        gpu_activity.push(&activity);
        let mut sample_performance_gate = performance_gate;
        sample_performance_gate = gpu_performance_gate_with_gaea_app_speedup(
            sample_performance_gate,
            &gpu_performance_limits,
            parsed.as_ref(),
            lhs_backend,
            rhs_backend,
        );
        let bridge_speedup_diagnostic = bridge_speedup_diagnostic_view(
            &gpu_performance_limits,
            parsed.as_ref(),
            lhs_backend,
            rhs_backend,
        );
        if cli.has("require-gpu-active")
            && activity.get("active").and_then(Value::as_bool) != Some(true)
        {
            sample_performance_gate =
                gpu_performance_gate_with_required_activity(sample_performance_gate, &activity);
        }
        let performance_passed = !gpu_performance_gate_failed(&sample_performance_gate);
        let compare_passed = passed && output.status_code == 0;
        let sample_passed = compare_passed && performance_passed;
        let sample_extra_flags = if !compare_passed {
            vec![
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ]
        } else if !performance_passed {
            vec!["--require-gpu-active"]
        } else {
            Vec::new()
        };
        let sample_params_json = params.to_json();
        let sample_next_focused_command = (!sample_passed).then(|| {
            gpu_sweep_tool_command_from_params(
                lhs_backend,
                rhs_backend,
                cli,
                Some(&sample_params_json),
                &sample_extra_flags,
            )
        });
        let sample_diagnosis = gpu_sweep_sample_diagnosis(
            lhs_backend,
            rhs_backend,
            parsed.as_ref(),
            compare_passed,
            exact,
            performance_passed,
            &sample_performance_gate,
            &bridge_speedup_diagnostic,
            &activity,
            &gpu_performance_limits,
            sample_next_focused_command.as_deref(),
        );
        if sample_passed {
            pass_count += 1;
        } else {
            failure_count += 1;
            if !compare_passed {
                gpu_compare_failure_count += 1;
            }
            if !performance_passed {
                performance_gate_failure_count += 1;
                if first_performance_gate_failure.is_none() {
                    first_performance_gate_failure = Some(json!({
                        "index": params.index,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "performance_gate": sample_performance_gate,
                        "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
                        "gpu_activity": activity,
                        "diagnosis": sample_diagnosis,
                    }));
                }
            }
            if first_failure.is_none() {
                first_failure = Some(json!({
                    "index": params.index,
                    "status": output.status_code,
                    "stdout": stdout_path,
                    "stderr": stderr_path,
                    "params": params.to_json(),
                    "exact": exact,
                    "performance_gate": sample_performance_gate,
                    "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
                    "gpu_activity": activity,
                    "summary": parsed.as_ref().and_then(summary_view),
                    "diagnosis": sample_diagnosis,
                }));
            }
        }
        samples.push(json!({
            "index": params.index,
            "status_kind": if sample_passed {
                "passed"
            } else if !compare_passed {
                "gpu_threshold_failure"
            } else {
                "gpu_performance_gate_failure"
            },
            "command": preview,
            "status": output.status_code,
            "passed": sample_passed,
            "compare_passed": compare_passed,
            "exact": exact,
            "performance_passed": performance_passed,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "params": params.to_json(),
            "preflight": preflight_summary,
            "timing": parsed.as_ref().and_then(backend_compare_timing_view),
            "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
            "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
            "gpu_activity": activity,
            "gpu_performance_gate": sample_performance_gate,
            "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
            "diagnosis": sample_diagnosis,
            "next_focused_command": sample_next_focused_command,
            "summary": parsed.as_ref().and_then(summary_view),
        }));
        if failure_count > 0 && !cli.has("keep-going") {
            break;
        }
    }
    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let stop_reason = if failure_count > 0 && !cli.has("keep-going") {
        "first_failure"
    } else if oracle_gap_count > 0 && !cli.has("keep-going") {
        "oracle_contract_gap"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };
    let all_passed = !samples.is_empty()
        && pass_count == samples.len()
        && failure_count == 0
        && oracle_gap_count == 0;
    let next_focused_command = gpu_sweep_next_focused_command(
        lhs_backend,
        rhs_backend,
        cli,
        first_failure.as_ref(),
        first_performance_gate_failure.as_ref(),
        first_oracle_gap.as_ref(),
    );
    let next_min_focused_cargo_run = gpu_sweep_next_min_focused_cargo_run(
        &ctx.cunning_core_manifest,
        lhs_backend,
        rhs_backend,
        cli,
        first_failure.as_ref(),
        first_performance_gate_failure.as_ref(),
        first_oracle_gap.as_ref(),
    );
    let engineering_report = gpu_sweep_engineering_report(
        all_passed,
        pass_count,
        failure_count,
        gpu_compare_failure_count,
        performance_gate_failure_count,
        oracle_gap_count,
        first_failure.as_ref(),
        first_performance_gate_failure.as_ref(),
        first_oracle_gap.as_ref(),
        &next_focused_command,
        &next_min_focused_cargo_run,
        &gpu_performance_limits,
    );

    let payload = json!({
        "mode": "executed",
        "command": "gpu-sweep",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "lhs_backend": lhs_backend,
        "rhs_backend": rhs_backend,
        "execution_roles": gpu_sweep_execution_roles(lhs_backend, rhs_backend, cli),
        "native_preflight": native_preflight,
        "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
        "require_gpu_active": cli.has("require-gpu-active"),
        "fresh_bridge_cache": cli.has("fresh-bridge-cache"),
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "pass_count": pass_count,
        "failure_count": failure_count,
        "gpu_compare_failure_count": gpu_compare_failure_count,
        "performance_gate_failure_count": performance_gate_failure_count,
        "oracle_gap_count": oracle_gap_count,
        "all_passed": all_passed,
        "seconds": seconds,
        "tolerance": {
            "mean_abs_norm_limit": mean_abs_norm_limit,
            "rmse_norm_limit": rmse_norm_limit,
            "max_abs_norm_limit": max_abs_norm_limit,
            "require_exact": cli.has("require-exact")
        },
        "timing_summary": {
            "preflight": preflight_timing.to_json(),
            "gpu": gpu_timing.to_json(),
        },
        "gpu_profile_summary": {
            "preflight": preflight_gpu_profile.to_json(),
            "gpu": gpu_profile.to_json(),
        },
        "gpu_activity_summary": gpu_activity.to_json(),
        "gpu_performance_limits": gpu_performance_limits.to_json(),
        "performance_policy": {
            "correctness_oracle": "GaeaBridge raw buffers",
            "speed_baseline": "Measured Gaea desktop app cook time",
            "bridge_elapsed": "diagnostic_only"
        },
        "first_failure": first_failure,
        "first_performance_gate_failure": first_performance_gate_failure,
        "first_oracle_gap": first_oracle_gap,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": next_min_focused_cargo_run,
        "engineering_report": engineering_report,
        "samples": samples,
        "truth_rule": "gpu-sweep validates the local GPU or hybrid backend against GaeaBridge for correctness. Performance gates use measured Gaea desktop app cook time, not Bridge elapsed time."
    });
    let summary_path = run_dir.join("gpu_sweep_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if failure_count > 0 || oracle_gap_count > 0 {
        return Err(format!(
            "Mountain GPU sweep found {failure_count} GPU failed sample(s), including {performance_gate_failure_count} performance gate failure(s), and {oracle_gap_count} oracle contract gap sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_preview(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-preview");
    }
    let samples = optional_usize_flag(cli, "samples")?.unwrap_or(8);
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let repeat = optional_u32_flag(cli, "repeat")?.unwrap_or(4).max(1);
    let preview_axis = optional_u32_flag(cli, "preview-axis")?
        .unwrap_or(129)
        .max(2);
    let preview_ms_budget = optional_f64_flag(cli, "preview-ms-budget")?.unwrap_or(100.0);
    let prewarm = cli.has("prewarm");

    if !cli.run() {
        let mut rng = SweepRng::new(rng_seed);
        let commands = (0..samples.min(16))
            .map(|index| {
                let params = mountain_sweep_params(cli, &mut rng, index)?;
                Ok(json!({
                    "index": params.index,
                    "params": params.to_json(),
                    "command": command_preview(&mountain_gpu_preview_profile_command(
                        ctx,
                        cli,
                        &params,
                        repeat,
                        preview_axis,
                    )),
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "gpu-preview",
                "node": "Mountain",
                "samples": samples,
                "repeat": repeat,
                "preview_axis": preview_axis,
                "preview_ms_budget": preview_ms_budget,
                "prewarm": prewarm,
                "commands": commands,
                "note": "Pass --run to execute Mountain GPU preview latency probes."
            }),
        );
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_preview").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut sample_reports = Vec::new();
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let mut max_warm_total_ms = 0.0f64;
    let mut max_warm_handle_ms = 0.0f64;
    let mut max_warm_preview_read_ms = 0.0f64;
    for index in 0..samples {
        let params = mountain_sweep_params(cli, &mut rng, index)?;
        let command = mountain_gpu_preview_profile_command(ctx, cli, &params, repeat, preview_axis);
        let preview = command_preview(&command);
        let output = run_capture_allow_failure(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!("{:04}_stdout.json", params.index));
        let stderr_path = run_dir.join(format!("{:04}_stderr.txt", params.index));
        write_text(&stdout_path, &stdout_text)?;
        write_text(&stderr_path, &output.stderr)?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let stats = parsed
            .as_ref()
            .map(gpu_preview_profile_stats)
            .unwrap_or_default();
        max_warm_total_ms = max_warm_total_ms.max(stats.warm_total_ms);
        max_warm_handle_ms = max_warm_handle_ms.max(stats.warm_handle_ms);
        max_warm_preview_read_ms = max_warm_preview_read_ms.max(stats.warm_preview_read_ms);
        let passed = output.status_code == 0
            && stats.gpu_resident
            && stats.warm_total_ms <= preview_ms_budget
            && (repeat <= 1
                || (stats.preview_hash_count > 1
                    && stats.handle_identity_count > 1
                    && stats.warm_changed_from_previous));
        if passed {
            pass_count += 1;
        } else {
            failure_count += 1;
        }
        sample_reports.push(json!({
            "index": params.index,
            "params": params.to_json(),
            "command": preview,
            "status": output.status_code,
            "passed": passed,
            "stdout": path_text(&stdout_path),
            "stderr": path_text(&stderr_path),
            "warm_total_ms": stats.warm_total_ms,
            "warm_handle_ms": stats.warm_handle_ms,
            "warm_preview_read_ms": stats.warm_preview_read_ms,
            "gpu_resident": stats.gpu_resident,
            "preview_hash_count": stats.preview_hash_count,
            "handle_identity_count": stats.handle_identity_count,
            "warm_changed_from_previous": stats.warm_changed_from_previous,
            "readback_count": stats.readback_count,
            "dispatch_count": stats.dispatch_count,
            "submit_count": stats.submit_count,
        }));
    }
    let summary = json!({
        "command": "gpu-preview",
        "node": "Mountain",
        "artifact_dir": path_text(&run_dir),
        "samples": samples,
        "pass_count": pass_count,
        "failure_count": failure_count,
        "all_passed": failure_count == 0,
        "repeat": repeat,
        "preview_axis": preview_axis,
        "preview_ms_budget": preview_ms_budget,
        "prewarm": prewarm,
        "max_warm_total_ms": max_warm_total_ms,
        "max_warm_handle_ms": max_warm_handle_ms,
        "max_warm_preview_read_ms": max_warm_preview_read_ms,
        "elapsed_ms": started_at.elapsed().as_secs_f64() * 1000.0,
        "samples_detail": sample_reports,
        "truth_rule": "gpu-preview measures interactive preview latency only. Bridge remains the final raw-buffer oracle."
    });
    let summary_path = run_dir.join("gpu_preview_summary.json");
    write_pretty_json(&summary_path, &summary)?;
    print_value(cli.json(), &summary);
    if failure_count > 0 && cli.has("require-all-pass") {
        return Err(format!(
            "Mountain GPU preview sweep found {failure_count} failing sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_candidate_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-candidate-sweep");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?.unwrap_or_else(|| {
        if seconds.is_some() {
            1_000_000
        } else {
            5
        }
    });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let candidates = gpu_candidate_backends(cli)?;
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    let native_preflight = !cli.has("skip-native-preflight") && backend_name_is_bridge(rhs_backend);
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(1.0e-4);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(2.0e-4);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(2.0e-3);
    let style_cycle = style_choices(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let mut commands = Vec::new();
        for index in 0..preview_count {
            let params =
                mountain_candidate_sweep_params(cli, &mut preview_rng, index, &style_cycle)?;
            let preflight = native_preflight.then(|| {
                command_preview(&mountain_native_bridge_preflight_command(ctx, cli, &params))
            });
            let candidate_commands = candidates
                .iter()
                .map(|candidate| {
                    json!({
                        "backend": candidate,
                        "command": command_preview(&mountain_gpu_sweep_command(
                            ctx,
                            cli,
                            &params,
                            candidate,
                            rhs_backend,
                            mean_abs_norm_limit,
                            rmse_norm_limit,
                            max_abs_norm_limit,
                        )),
                    })
                })
                .collect::<Vec<_>>();
            commands.push(json!({
                "index": params.index,
                "style_family": mountain_style_family(&params.style),
                "params": params.to_json(),
                "preflight": preflight,
                "candidates": candidate_commands,
            }));
        }
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-candidate-sweep",
            "node": "Mountain",
            "candidate_backends": candidates,
            "rhs_backend": rhs_backend,
            "native_preflight": native_preflight,
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "style_choices": style_cycle,
            "tolerance": {
                "mean_abs_norm_limit": mean_abs_norm_limit,
                "rmse_norm_limit": rmse_norm_limit,
                "max_abs_norm_limit": max_abs_norm_limit,
                "require_exact": cli.has("require-exact")
            },
            "commands": commands,
            "note": "Pass --run to execute candidate classification against the Bridge oracle."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_candidate_sweep").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut samples = Vec::new();
    let mut candidate_stats: BTreeMap<String, CandidateSweepStats> = BTreeMap::new();
    let mut oracle_gap_count = 0usize;
    let mut candidate_run_count = 0usize;
    let mut candidate_pass_count = 0usize;
    let mut candidate_failure_count = 0usize;
    let mut style_family_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_failure = None;
    let mut first_oracle_gap = None;

    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_candidate_sweep_params(cli, &mut rng, index, &style_cycle)?;
        let style_family = mountain_style_family(&params.style);
        *style_family_counts
            .entry(style_family.to_string())
            .or_insert(0) += 1;
        let mut preflight_summary = None;
        if native_preflight {
            let command = mountain_native_bridge_preflight_command(ctx, cli, &params);
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path = run_dir.join(format!("{:04}_preflight_stdout.json", params.index));
            let stderr_path = run_dir.join(format!("{:04}_preflight_stderr.txt", params.index));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            let preflight = json!({
                "command": preview,
                "status": output.status_code,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "summary": parsed.as_ref().and_then(summary_view),
            });
            if !(exact && output.status_code == 0) {
                oracle_gap_count += 1;
                if first_oracle_gap.is_none() {
                    first_oracle_gap = Some(json!({
                        "index": params.index,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "summary": parsed.as_ref().and_then(summary_view),
                    }));
                }
                samples.push(json!({
                    "index": params.index,
                    "style_family": style_family,
                    "status_kind": "oracle_contract_gap",
                    "params": params.to_json(),
                    "preflight": preflight,
                    "candidates": [],
                }));
                if !cli.has("keep-going") {
                    break;
                }
                continue;
            }
            preflight_summary = Some(preflight);
        }

        let mut candidate_results = Vec::new();
        for candidate in &candidates {
            candidate_run_count += 1;
            let command = mountain_gpu_sweep_command(
                ctx,
                cli,
                &params,
                candidate,
                rhs_backend,
                mean_abs_norm_limit,
                rmse_norm_limit,
                max_abs_norm_limit,
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path =
                run_dir.join(format!("{:04}_{}_stdout.json", params.index, candidate));
            let stderr_path = run_dir.join(format!("{:04}_{}_stderr.txt", params.index, candidate));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let passed = parsed.as_ref().map(backend_compare_passed).unwrap_or(false);
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            let status_kind = classify_gpu_candidate_result(candidate, &params, passed, exact);
            if passed && output.status_code == 0 {
                candidate_pass_count += 1;
            } else {
                candidate_failure_count += 1;
                if first_failure.is_none() {
                    first_failure = Some(json!({
                        "index": params.index,
                        "backend": candidate,
                        "status_kind": status_kind,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "summary": parsed.as_ref().and_then(summary_view),
                    }));
                }
            }
            candidate_stats.entry(candidate.clone()).or_default().push(
                style_family,
                &status_kind,
                passed,
                exact,
                parsed.as_ref(),
            );
            candidate_results.push(json!({
                "backend": candidate,
                "status_kind": status_kind,
                "command": preview,
                "status": output.status_code,
                "passed": passed,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "summary": parsed.as_ref().and_then(summary_view),
            }));
        }
        samples.push(json!({
            "index": params.index,
            "style_family": style_family,
            "params": params.to_json(),
            "preflight": preflight_summary,
            "candidates": candidate_results,
        }));
    }

    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let stop_reason = if oracle_gap_count > 0 && !cli.has("keep-going") {
        "oracle_contract_gap"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };
    let candidate_summary = candidate_stats
        .iter()
        .map(|(backend, stats)| {
            (
                backend.clone(),
                stats.to_json(candidate_name_is_shader_ridge(backend)),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let full_style_family_coverage = style_family_counts.contains_key("basic_no_pe")
        && style_family_counts.contains_key("pe_style");
    let payload = json!({
        "mode": "executed",
        "command": "gpu-candidate-sweep",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "candidate_backends": candidates,
        "rhs_backend": rhs_backend,
        "native_preflight": native_preflight,
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "candidate_run_count": candidate_run_count,
        "candidate_pass_count": candidate_pass_count,
        "candidate_failure_count": candidate_failure_count,
        "oracle_gap_count": oracle_gap_count,
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "style_choices": style_cycle,
        "style_family_counts": style_family_counts,
        "full_style_family_coverage": full_style_family_coverage,
        "tolerance": {
            "mean_abs_norm_limit": mean_abs_norm_limit,
            "rmse_norm_limit": rmse_norm_limit,
            "max_abs_norm_limit": max_abs_norm_limit,
            "require_exact": cli.has("require-exact")
        },
        "candidate_summary": candidate_summary,
        "first_failure": first_failure,
        "first_oracle_gap": first_oracle_gap,
        "samples": samples,
        "truth_rule": "GPU candidate promotion is judged only against GaeaBridge; Native CPU/live paths are preflight/localization helpers."
    });
    let summary_path = run_dir.join("gpu_candidate_sweep_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (candidate_failure_count > 0 || oracle_gap_count > 0) {
        return Err(format!(
            "Mountain GPU candidate sweep found {candidate_failure_count} candidate failed run(s) and {oracle_gap_count} oracle gap sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_stage_audit(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-stage-audit");
    }
    let command = mountain_gpu_stage_audit_command(ctx, cli);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-stage-audit",
            "node": "Mountain",
            "stage": cli.flag("stage").unwrap_or("all"),
            "command_line": command_preview(&command),
            "note": "Pass --run to execute the WGSL exact-upload toggle audit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_stage_audit").join(format!(
        "mountain_{}_stage{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("stage").unwrap_or("all"))
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let preview = command_preview(&command);
    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let stdout_path = run_dir.join("stdout.json");
    let stderr_path = run_dir.join("stderr.txt");
    write_text(&stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
    let all_exact = parsed
        .as_ref()
        .and_then(|value| value.get("all_exact"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let payload = json!({
        "mode": "executed",
        "command": "gpu-stage-audit",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "status": output.status_code,
        "all_exact": all_exact,
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": gpu_stage_audit_summary_view(parsed.as_ref()),
        "truth_rule": "This audit disables one exact upload at a time and compares the WGSL stage against the already Bridge-aligned CPU reference stage."
    });
    let summary_path = run_dir.join("gpu_stage_audit_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-exact") && !all_exact {
        return Err(format!(
            "Mountain GPU stage audit found non-exact WGSL stage(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_substrate(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-substrate");
    }
    let command = mountain_gpu_substrate_command(ctx, cli);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-substrate",
            "node": "Mountain",
            "source_resolution": cli.flag("source-resolution").unwrap_or("16x12"),
            "target_resolution": cli.flag("target-resolution").unwrap_or("4x3"),
            "layers": cli.flag("layers").unwrap_or("4"),
            "command_line": command_preview(&command),
            "note": "Pass --run to execute the PE GPU substrate compare and write artifacts."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_substrate").join(format!(
        "mountain_{}_{}to{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("source-resolution").unwrap_or("default")),
        sanitize_filename(cli.flag("target-resolution").unwrap_or("default"))
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let preview = command_preview(&command);
    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let stdout_path = run_dir.join("stdout.json");
    let stderr_path = run_dir.join("stderr.txt");
    write_text(&stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
    let failed = parsed
        .as_ref()
        .and_then(|value| value.get("failed"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let reports = parsed
        .as_ref()
        .and_then(|value| value.get("reports"))
        .and_then(Value::as_array);
    let report_count = reports.map(|items| items.len()).unwrap_or(0);
    let failed_report_count = reports
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("passed").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(report_count.max(1));
    let payload = json!({
        "mode": "executed",
        "command": "gpu-substrate",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "status": output.status_code,
        "failed": failed,
        "report_count": report_count,
        "failed_report_count": failed_report_count,
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": gpu_substrate_summary_view(parsed.as_ref()),
        "truth_rule": "This command proves low-level PE GPU substrate contracts against the CPU reference layer that was aligned to Bridge; Bridge remains the final node oracle."
    });
    let summary_path = run_dir.join("gpu_substrate_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (failed || output.status_code != 0 || failed_report_count > 0)
    {
        return Err(format!(
            "Mountain GPU substrate compare found {failed_report_count} failed report(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_wave(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-wave");
    }
    let gpu_performance_limits = GpuPerformanceLimits::from_cli(cli)?;
    let command = mountain_gpu_wave_command(ctx, cli);
    if !cli.run() {
        let dry_run_case = cli.flag("case").unwrap_or("old_baseline");
        let resident_min_level_diagnosis =
            resident_min_level_diagnostics_view(&ctx.cunning_core_manifest, cli, None, None);
        let next_focused_command = gpu_wave_focused_command_with_context(
            cli,
            dry_run_case,
            None,
            &["--require-gpu-active"],
        );
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-wave",
            "node": "Mountain",
            "case": cli.flag("case").unwrap_or("all"),
            "epsilon": cli.flag("epsilon").unwrap_or("0"),
            "execution_roles": gpu_wave_execution_roles(cli),
            "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "gpu_performance_limits": gpu_performance_limits.to_json(),
            "gpu_runtime_policy_threshold": gpu_performance_limits.policy_gpu_cpu_ratio_threshold(),
            "command_line": command_preview(&command),
            "next_focused_command": next_focused_command,
            "resident_min_level_diagnosis": resident_min_level_diagnosis,
            "next_min_focused_cargo_run": mountain_gpu_wave_cargo_command_with_context(
                &ctx.cunning_core_manifest,
                cli,
                dry_run_case,
                None,
                &["--require-gpu-active"],
            ),
            "diagnostic_output": {
                "field": "migration_blocker",
                "purpose": "Classifies Mountain GPU correctness blockers as path_commit_integrated_mismatch or scalar_exact_mismatch and emits a direct cargo run repro command."
            },
            "engineering_fields": [
                "engineering_report",
                "first_mismatch",
                "bridge_oracle_gate",
                "gpu_activity_status",
                "next_commands"
            ],
            "note": "Pass --run to execute the Mountain CPU-live versus GPU-wave-writeback compare and write artifacts."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_wave").join(format!(
        "mountain_{}_{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("case").unwrap_or("all"))
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let preview = command_preview(&command);
    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let stdout_path = run_dir.join("stdout.json");
    let stderr_path = run_dir.join("stderr.txt");
    write_text(&stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
    let failed = parsed
        .as_ref()
        .and_then(|value| value.get("failed"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let case_count = parsed
        .as_ref()
        .and_then(|value| value.get("case_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed_case_count = parsed
        .as_ref()
        .and_then(|value| value.get("cases"))
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter(|case| case.get("passed").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(case_count.max(1) as usize);
    let gpu_performance_gate = gpu_wave_performance_gate_view(
        parsed.as_ref(),
        &gpu_performance_limits,
        cli.has("gpu-exact-barrier"),
        mountain_gpu_wave_policy(cli).as_deref().unwrap_or("force"),
    );
    let gpu_performance_gate_failed = gpu_performance_gate_failed(&gpu_performance_gate);
    let summary = gpu_wave_summary_view(
        parsed.as_ref(),
        cli.has("gpu-exact-barrier"),
        &gpu_performance_limits,
    );
    let runtime_policy = gpu_wave_runtime_policy_view(parsed.as_ref(), &gpu_performance_limits);
    let runtime_policy_path = run_dir.join("gpu_runtime_policy.json");
    if let Some(policy) = runtime_policy.as_ref() {
        write_pretty_json(&runtime_policy_path, policy)?;
    }
    let runtime_policy_path_value = runtime_policy
        .as_ref()
        .map(|_| runtime_policy_path.display().to_string());
    let diagnosis = gpu_wave_diagnosis_view(
        parsed.as_ref(),
        summary.as_ref(),
        &gpu_performance_gate,
        runtime_policy.as_ref(),
        cli,
        output.status_code,
        failed,
        failed_case_count,
    );
    let resident_min_level_diagnosis = resident_min_level_diagnostics_view(
        &ctx.cunning_core_manifest,
        cli,
        parsed.as_ref(),
        summary.as_ref(),
    );
    let migration_blocker = mountain_gpu_migration_blocker_view(
        &ctx.cunning_core_manifest,
        parsed.as_ref(),
        summary.as_ref(),
        cli,
        output.status_code,
        failed,
        failed_case_count,
    );
    let next_min_focused_cargo_run = migration_blocker
        .get("next_min_focused_cargo_run")
        .or_else(|| migration_blocker.get("next_cargo_run_command"))
        .cloned();
    let next_focused_command = diagnosis.get("next_focused_command").cloned();
    let first_mismatch = diagnosis.get("first_mismatch").cloned();
    let engineering_report = gpu_wave_engineering_report(
        &diagnosis,
        &migration_blocker,
        &gpu_performance_gate,
        runtime_policy.as_ref(),
        next_min_focused_cargo_run.as_ref(),
        Some(&resident_min_level_diagnosis),
    );
    let payload = json!({
        "mode": "executed",
        "command": "gpu-wave",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "execution_roles": gpu_wave_execution_roles(cli),
        "status": output.status_code,
        "failed": failed,
        "case_count": case_count,
        "failed_case_count": failed_case_count,
        "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "gpu_performance_limits": gpu_performance_limits.to_json(),
        "gpu_performance_gate": gpu_performance_gate,
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": summary,
        "runtime_policy": runtime_policy,
        "runtime_policy_path": runtime_policy_path_value,
        "first_mismatch": first_mismatch,
        "resident_min_level_diagnosis": resident_min_level_diagnosis,
        "diagnosis": diagnosis,
        "migration_blocker": migration_blocker,
        "engineering_report": engineering_report,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": next_min_focused_cargo_run,
        "truth_rule": "This command checks the live Mountain GPU wave-writeback path against the Bridge-aligned CPU path; Bridge remains the node oracle and GPU float tails are bounded by --epsilon."
    });
    let summary_path = run_dir.join("gpu_wave_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (failed || output.status_code != 0 || failed_case_count > 0) {
        return Err(format!(
            "Mountain GPU wave compare found {failed_case_count} failed case(s). See '{}'.",
            summary_path.display()
        ));
    }
    if gpu_performance_gate_failed {
        return Err(format!(
            "Mountain GPU wave performance gate failed. See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gpu_resident_replay(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-resident-replay");
    }
    let command = mountain_gpu_resident_replay_command(ctx, cli);
    if !cli.run() {
        let next_focused_command =
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]);
        let resident_min_level_diagnosis =
            resident_min_level_diagnostics_view(&ctx.cunning_core_manifest, cli, None, None);
        let resident_next_cargo_run = resident_min_level_diagnosis
            .pointer("/next_commands/primary/command")
            .cloned();
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-resident-replay",
            "node": "Mountain",
            "case": cli.flag("case").unwrap_or("old_baseline"),
            "resident_wave_count": cli.flag("resident-wave-count").unwrap_or("1"),
            "resident_min_level": cli.flag("resident-min-level").unwrap_or("4"),
            "epsilon": cli.flag("epsilon").unwrap_or("0.0001"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "command_line": command_preview(&command),
            "next_focused_command": next_focused_command,
            "resident_min_level_diagnosis": resident_min_level_diagnosis,
            "next_min_focused_cargo_run": resident_next_cargo_run,
            "note": "Pass --run to execute the Mountain CPU replay versus GPU resident replay stage compare."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_resident_replay").join(format!(
        "mountain_{}_{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("case").unwrap_or("old_baseline"))
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let preview = command_preview(&command);
    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let stdout_path = run_dir.join("stdout.json");
    let stderr_path = run_dir.join("stderr.txt");
    write_text(&stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
    let failed = parsed
        .as_ref()
        .and_then(|value| value.get("failed"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let failed_report_count = parsed
        .as_ref()
        .and_then(|value| value.get("reports"))
        .and_then(Value::as_array)
        .map(|reports| {
            reports
                .iter()
                .filter(|report| report.get("passed").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(1);
    let summary = gpu_resident_replay_summary_view(parsed.as_ref());
    let diagnosis = gpu_resident_replay_diagnosis_view(
        parsed.as_ref(),
        summary.as_ref(),
        cli,
        output.status_code,
        failed,
        failed_report_count,
    );
    let resident_min_level_diagnosis = resident_min_level_diagnostics_view(
        &ctx.cunning_core_manifest,
        cli,
        parsed.as_ref(),
        summary.as_ref(),
    );
    let engineering_report =
        gpu_resident_replay_engineering_report(&diagnosis, &resident_min_level_diagnosis);
    let next_focused_command = diagnosis.get("next_focused_command").cloned();
    let resident_next_cargo_run = resident_min_level_diagnosis
        .pointer("/next_commands/primary/command")
        .cloned();
    let payload = json!({
        "mode": "executed",
        "command": "gpu-resident-replay",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "status": output.status_code,
        "failed": failed,
        "failed_report_count": failed_report_count,
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": summary,
        "pe_profile": mountain_pe_profile_view(&output.stderr),
        "resident_min_level_diagnosis": resident_min_level_diagnosis,
        "diagnosis": diagnosis,
        "engineering_report": engineering_report,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": resident_next_cargo_run,
        "truth_rule": "This command localizes the live Mountain resident GPU replay against the CPU replay; Bridge remains the final node oracle."
    });
    let summary_path = run_dir.join("gpu_resident_replay_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (failed || output.status_code != 0 || failed_report_count > 0)
    {
        return Err(format!(
            "Mountain GPU resident replay compare found {failed_report_count} failed report(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct MountainSweepParams {
    index: usize,
    style: String,
    bulk: String,
    reduce_details: bool,
    scale: f32,
    height: f32,
    seed: i32,
    x: f32,
    y: f32,
    terrain_width: f32,
    terrain_height: f32,
    resolution: u32,
}
