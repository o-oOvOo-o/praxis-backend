fn default_gaea_install_dir() -> PathBuf {
    PathBuf::from(r"F:\Gaea 2")
}

fn gaea_viewport_reverse_command(gaea_dir: &Path) -> Command {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &gaea_viewport_reverse_powershell(gaea_dir),
    ]);
    command
}

fn gaea_viewport_reverse_powershell(gaea_dir: &Path) -> String {
    let gaea_dir = escape_powershell_single_quoted(&path_text(gaea_dir));
    format!(
        r#"$ErrorActionPreference = 'Stop'
$gaeaDir = '{gaea_dir}'
$managed = Join-Path $gaeaDir 'Gaea.Viewport_Data\Managed'
$asmPath = Join-Path $managed 'Assembly-CSharp.dll'
$resolver = [System.ResolveEventHandler]{{ param($sender,$e)
    $name = ($e.Name -split ',')[0] + '.dll'
    $path = Join-Path $managed $name
    if (Test-Path $path) {{ return [System.Reflection.Assembly]::LoadFrom($path) }}
    return $null
}}
[AppDomain]::CurrentDomain.add_AssemblyResolve($resolver)
$asm = [System.Reflection.Assembly]::LoadFrom($asmPath)
$keywordSet = @(
    'TerrainData','Terrain','SetHeights','heightmapResolution','heightmapPixelError',
    'Texture2D','LoadRawTextureData','Apply','SetTexture','MeshFilter','MeshCollider',
    'Mesh','Renderer','Material','ProcGen','PlaneX','PreviewResolution',
    '_Displacement','_DisplacementTex','_Albedo','preventHiMesh','SetOptimization',
    'SetQuality','ResizeTerrain','SetTerrain','UpdateCollisionMesh'
)
$types = @()
foreach ($t in ($asm.GetTypes() | Sort-Object FullName)) {{
    $fields = @($t.GetFields('Public,NonPublic,Instance,Static,DeclaredOnly') | ForEach-Object {{
        [ordered]@{{ name=$_.Name; field_type=$_.FieldType.FullName; is_static=$_.IsStatic }}
    }})
    $methods = @($t.GetMethods('Public,NonPublic,Instance,Static,DeclaredOnly') | Where-Object {{ -not $_.IsSpecialName }} | ForEach-Object {{
        [ordered]@{{
            name=$_.Name
            return_type=$_.ReturnType.FullName
            parameters=@($_.GetParameters() | ForEach-Object {{ [ordered]@{{ name=$_.Name; parameter_type=$_.ParameterType.FullName }} }})
        }}
    }})
    $name = [string]$t.FullName
    if ($name -match 'Comms|ProcGen|PlaneX|ProceduralShape|PreviewResolution|Camera|Terrain|Mesh|Texture') {{
        $types += [ordered]@{{ full_name=$t.FullName; base_type=$t.BaseType.FullName; fields=$fields; methods=$methods }}
    }}
}}
$metadataHits = [ordered]@{{}}
foreach ($kw in $keywordSet) {{
    $hits = @()
    foreach ($t in $asm.GetTypes()) {{
        if ($t.FullName -like "*$kw*") {{ $hits += "TYPE $($t.FullName)" }}
        foreach ($f in $t.GetFields('Public,NonPublic,Instance,Static,DeclaredOnly')) {{
            if (($f.Name -like "*$kw*") -or ($f.FieldType.FullName -like "*$kw*")) {{
                $hits += "FIELD $($t.FullName)::$($f.Name) $($f.FieldType.FullName)"
            }}
        }}
        foreach ($m in $t.GetMethods('Public,NonPublic,Instance,Static,DeclaredOnly')) {{
            if (($m.Name -like "*$kw*") -or ($m.ToString() -like "*$kw*")) {{
                $hits += "METHOD $($t.FullName)::$($m.Name) $($m.ToString())"
            }}
        }}
    }}
    $metadataHits[$kw] = @($hits | Select-Object -First 120)
}}
function MethodCalls($typeName, $methodName) {{
    $t = $asm.GetType($typeName)
    if ($null -eq $t) {{ return @([ordered]@{{ error="missing_type"; type=$typeName; method=$methodName }}) }}
    $result = @()
    foreach ($m in ($t.GetMethods('Public,NonPublic,Instance,Static,DeclaredOnly') | Where-Object {{ $_.Name -eq $methodName }})) {{
        $body = $m.GetMethodBody()
        if ($null -eq $body) {{
            $result += [ordered]@{{ type=$typeName; method=$methodName; calls=@(); strings=@(); fields=@(); note='no_body' }}
            continue
        }}
        $il = $body.GetILAsByteArray()
        $calls = @()
        $fields = @()
        $strings = @()
        for ($i = 0; $i -lt $il.Length - 4; $i++) {{
            $op = $il[$i]
            if ($op -eq 0x28 -or $op -eq 0x6F) {{
                try {{
                    $tok = [BitConverter]::ToInt32($il, $i + 1)
                    $member = $m.Module.ResolveMethod($tok)
                    $calls += "$($member.DeclaringType.FullName)::$($member.Name)"
                }} catch {{}}
            }} elseif ($op -eq 0x7B -or $op -eq 0x7C -or $op -eq 0x7D -or $op -eq 0x7E -or $op -eq 0x80) {{
                try {{
                    $tok = [BitConverter]::ToInt32($il, $i + 1)
                    $member = $m.Module.ResolveField($tok)
                    $fields += "$($member.DeclaringType.FullName)::$($member.Name)"
                }} catch {{}}
            }} elseif ($op -eq 0x72) {{
                try {{
                    $tok = [BitConverter]::ToInt32($il, $i + 1)
                    $strings += $m.Module.ResolveString($tok)
                }} catch {{}}
            }}
        }}
        $result += [ordered]@{{
            type=$typeName
            method=$methodName
            calls=@($calls | Select-Object -Unique)
            fields=@($fields | Select-Object -Unique)
            strings=@($strings | Select-Object -Unique)
        }}
    }}
    return $result
}}
$methods = @()
$targets = @(
    @('Comms','Start'), @('Comms','HandleMessageReceived'), @('Comms','SetOptimization'),
    @('Comms','SetQuality'), @('Comms','ResizeTerrain'), @('Comms','SetTerrain'),
    @('Comms','EnsureTexture'), @('Comms','UpdateCollisionMesh'),
    @('ProcGen','Awake'), @('ProcGen','Set512'), @('ProcGen','Set1024'),
    @('ProcGen','Set2048'), @('ProcGen','Set4096'), @('ProcGen','ChangeMesh'),
    @('PlaneX','CreateMesh'), @('PlaneX','CreateVertices'), @('PlaneX','CreateTriangles'),
    @('PlaneX','CreateUVs')
)
foreach ($target in $targets) {{ $methods += MethodCalls $target[0] $target[1] }}
$assetStringEvidence = @()
$assetPaths = @((Join-Path $gaeaDir 'Gaea.Viewport_Data\data.unity3d'), (Join-Path $managed 'Assembly-CSharp.dll'))
foreach ($assetPath in $assetPaths) {{
    if (Test-Path $assetPath) {{
        $bytes = [System.IO.File]::ReadAllBytes($assetPath)
        $textUtf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
        $textAscii = [System.Text.Encoding]::ASCII.GetString($bytes)
        foreach ($kw in @('_DisplacementTex','_Displacement','_Albedo','ProcGen','PlaneX','UnityEngine.TerrainModule','TerrainData','SetHeights')) {{
            $assetStringEvidence += [ordered]@{{ path=$assetPath; keyword=$kw; found=($textUtf16.Contains($kw) -or $textAscii.Contains($kw)) }}
        }}
    }}
}}
$payload = [ordered]@{{
    gaea_dir=$gaeaDir
    managed_dir=$managed
    viewport_dll=$asmPath
    assembly_full_name=$asm.FullName
    inspected_types=$types
    metadata_hits=$metadataHits
    method_call_evidence=$methods
    asset_string_evidence=$assetStringEvidence
    terrain_api_absence=[ordered]@{{
        terrain_data_hits=@($metadataHits['TerrainData'])
        set_heights_hits=@($metadataHits['SetHeights'])
        heightmap_resolution_hits=@($metadataHits['heightmapResolution'])
        heightmap_pixel_error_hits=@($metadataHits['heightmapPixelError'])
    }}
}}
$payload | ConvertTo-Json -Depth 20
"#
    )
}

fn escape_powershell_single_quoted(text: &str) -> String {
    text.replace('\'', "''")
}

fn gaea_viewport_main_source_evidence(comms: &Path, b: &Path, viewport_area: &Path) -> Value {
    json!({
        "comms_cs": {
            "path": comms,
            "line_evidence": source_line_hits(comms, &[
                "internal static void SendTerrain",
                "HeightfieldByteSize",
                "BlockCopy",
                "ResizeTerrain",
                "PreventHiRes",
            ])
        },
        "b_cs": {
            "path": b,
            "line_evidence": source_line_hits(b, &[
                "internal static void TransmitTerrain",
                "Comms.ResizeTerrain",
                "Comms.SetHeight",
                "Comms.SendTerrain",
            ])
        },
        "viewport_area_cs": {
            "path": viewport_area,
            "line_evidence": source_line_hits(viewport_area, &[
                "ViewportQuality",
                "PreventHiRes",
                "Comms.Send",
            ])
        }
    })
}

fn source_line_hits(path: &Path, needles: &[&str]) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if needles.iter().any(|needle| line.contains(needle)) {
            hits.push(json!({
                "line": line_index + 1,
                "text": line.trim(),
            }));
        }
    }
    hits
}

fn gaea_viewport_conclusion(reflected: &Value) -> Value {
    let terrain_data_hits = reflected
        .pointer("/terrain_api_absence/terrain_data_hits")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let set_heights_hits = reflected
        .pointer("/terrain_api_absence/set_heights_hits")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let procgen_hits = reflected
        .pointer("/metadata_hits/ProcGen")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let planex_hits = reflected
        .pointer("/metadata_hits/PlaneX")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let displacement_hits = reflected
        .pointer("/method_call_evidence")
        .and_then(Value::as_array)
        .map(|methods| {
            methods
                .iter()
                .filter(|method| {
                    method
                        .get("strings")
                        .and_then(Value::as_array)
                        .map(|strings| {
                            strings
                                .iter()
                                .any(|value| value.as_str() == Some("_DisplacementTex"))
                        })
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    let asset_displacement_hits = reflected
        .get("asset_string_evidence")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("keyword").and_then(Value::as_str) == Some("_DisplacementTex")
                        && item.get("found").and_then(Value::as_bool) == Some(true)
                })
                .count()
        })
        .unwrap_or(0);
    json!({
        "classification": if terrain_data_hits == 0 && set_heights_hits == 0 && procgen_hits > 0 && planex_hits > 0 {
            "texture_displaced_fixed_quality_plane_mesh"
        } else {
            "needs_manual_review"
        },
        "terrain_data_api_hit_count": terrain_data_hits,
        "set_heights_hit_count": set_heights_hits,
        "procgen_hit_count": procgen_hits,
        "planex_hit_count": planex_hits,
        "displacement_texture_method_string_hit_count": displacement_hits,
        "displacement_texture_asset_string_hit_count": asset_displacement_hits,
        "evidence_summary": [
            "Assembly-CSharp metadata has ProcGen, PlaneX, and PreviewResolution tiers.",
            "Comms.ResizeTerrain switches mesh tiers and allocates raw height/color buffers.",
            "Comms.SetTerrain uploads raw height bytes to Texture2D and binds _DisplacementTex.",
            "No direct TerrainData/SetHeights/heightmapResolution/heightmapPixelError metadata evidence was found."
        ],
        "lod_interpretation": "Gaea viewport evidence points to fixed quality-tier mesh selection plus material displacement, not Unity Terrain quadtree LOD."
    })
}

fn gaea_viewport_report_markdown(payload: &Value) -> String {
    let classification = payload
        .pointer("/conclusion/classification")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let viewport_dll = payload
        .get("viewport_dll")
        .map(scalar_text)
        .unwrap_or_else(|| "unknown".to_string());
    let artifact_dir = payload
        .get("artifact_dir")
        .map(scalar_text)
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "# Gaea Viewport Reverse Summary\n\n\
        ## Classification\n\n\
        `{classification}`\n\n\
        ## Evidence\n\n\
        - Viewport DLL: `{viewport_dll}`\n\
        - Artifact dir: `{artifact_dir}`\n\
        - The Unity viewport metadata exposes `ProcGen`, `PlaneX`, and `PreviewResolution` tiers.\n\
        - The Unity viewport path uploads raw height bytes to a `Texture2D` and binds `_DisplacementTex`.\n\
        - No direct `TerrainData.SetHeights` or Unity terrain heightmap-resolution API was found in `Assembly-CSharp.dll` metadata.\n\n\
        ## Cunning Direction\n\n\
        Keep the full-resolution height texture and decouple viewport geometry density from source resolution. Use fixed or view-dependent display mesh tiers with GPU displacement; do not rebuild full-resolution CPU meshes for interactive viewport display.\n"
    )
}

#[derive(Debug)]
struct RunOutput {
    status_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn run_capture(mut command: Command) -> Result<RunOutput, String> {
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Failed to run '{}': {error}", command_preview(&command)))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status_code = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        return Err(format!(
            "Command failed with status {status_code}: {}\nSTDERR:\n{stderr}\nSTDOUT:\n{stdout}",
            command_preview(&command)
        ));
    }
    Ok(RunOutput {
        status_code,
        stdout,
        stderr,
        timed_out: false,
    })
}

fn run_capture_allow_failure(mut command: Command) -> Result<RunOutput, String> {
    let preview = command_preview(&command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);
    let start = Instant::now();
    let mut next_heartbeat = start + CAPTURE_HEARTBEAT_INTERVAL;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to poll '{preview}': {error}"))?
        {
            return Ok(RunOutput {
                status_code: status.code().unwrap_or(-1),
                stdout: collect_pipe_reader(stdout_reader, &preview, "stdout")?,
                stderr: collect_pipe_reader(stderr_reader, &preview, "stderr")?,
                timed_out: false,
            });
        }
        let now = Instant::now();
        if now >= next_heartbeat {
            eprintln!(
                "capture heartbeat: elapsed={}s command={}",
                start.elapsed().as_secs(),
                preview
            );
            next_heartbeat = now + CAPTURE_HEARTBEAT_INTERVAL;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_capture_allow_failure_filebacked(
    mut command: Command,
    run_dir: &Path,
    index: usize,
) -> Result<RunOutput, String> {
    let preview = command_preview(&command);
    let stdout_tmp = run_dir.join(format!("command_{index}_stdout.raw.tmp"));
    let stderr_tmp = run_dir.join(format!("command_{index}_stderr.raw.tmp"));
    let stdout_file = fs::File::create(&stdout_tmp)
        .map_err(|error| format!("Failed to create '{}': {error}", stdout_tmp.display()))?;
    let stderr_file = fs::File::create(&stderr_tmp)
        .map_err(|error| format!("Failed to create '{}': {error}", stderr_tmp.display()))?;
    let mut child = command
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let start = Instant::now();
    let mut next_heartbeat = start + CAPTURE_HEARTBEAT_INTERVAL;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to poll '{preview}': {error}"))?
        {
            break status;
        }
        let now = Instant::now();
        if now >= next_heartbeat {
            eprintln!(
                "capture heartbeat: elapsed={}s command={}",
                start.elapsed().as_secs(),
                preview
            );
            next_heartbeat = now + CAPTURE_HEARTBEAT_INTERVAL;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let stdout = fs::read(&stdout_tmp)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|error| format!("Failed to read '{}': {error}", stdout_tmp.display()))?;
    let stderr = fs::read(&stderr_tmp)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|error| format!("Failed to read '{}': {error}", stderr_tmp.display()))?;
    let _ = fs::remove_file(&stdout_tmp);
    let _ = fs::remove_file(&stderr_tmp);
    Ok(RunOutput {
        status_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        timed_out: false,
    })
}

fn run_capture_allow_failure_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<RunOutput, String> {
    let preview = command_preview(&command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to poll '{preview}': {error}"))?
        {
            return Ok(RunOutput {
                status_code: status.code().unwrap_or(-1),
                stdout: collect_pipe_reader(stdout_reader, &preview, "stdout")?,
                stderr: collect_pipe_reader(stderr_reader, &preview, "stderr")?,
                timed_out: false,
            });
        }
        if start.elapsed() >= timeout {
            kill_process_tree(child.id());
            let _ = child.kill();
            let status = child
                .wait()
                .map_err(|error| format!("Failed to collect timed-out '{preview}': {error}"))?;
            return Ok(RunOutput {
                status_code: status.code().unwrap_or(-1),
                stdout: collect_pipe_reader(stdout_reader, &preview, "stdout")?,
                stderr: collect_pipe_reader(stderr_reader, &preview, "stderr")?,
                timed_out: true,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn spawn_pipe_reader<R>(mut reader: R) -> thread::JoinHandle<Result<String, String>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to drain process pipe: {error}"))?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    })
}

fn collect_pipe_reader(
    reader: Option<thread::JoinHandle<Result<String, String>>>,
    preview: &str,
    stream: &str,
) -> Result<String, String> {
    let Some(reader) = reader else {
        return Ok(String::new());
    };
    reader
        .join()
        .map_err(|_| format!("Failed to join {stream} reader for '{preview}'"))?
        .map_err(|error| format!("{error} while running '{preview}'"))
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn kill_process_tree(_pid: u32) {}

fn run_and_write_jsonish(mut command: Command, path: &Path) -> Result<(), String> {
    let preview = command_preview(&command);
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json_text = extract_jsonish(&stdout).unwrap_or(stdout);
    fs::write(path, &json_text)
        .map_err(|error| format!("Failed to write '{}': {error}", path.display()))?;
    let stderr_path = path.with_extension("stderr.txt");
    fs::write(&stderr_path, stderr)
        .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "Command failed with status {}: {preview}. stdout='{}' stderr='{}'",
            output.status.code().unwrap_or(-1),
            path.display(),
            stderr_path.display()
        ));
    }
    Ok(())
}

fn gaea_swarm_command(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
) -> Command {
    let mut command = Command::new(swarm_exe);
    command
        .arg("--Filename")
        .arg(terrain)
        .arg("--node")
        .arg(node_id.to_string())
        .arg("--resolution")
        .arg(resolution.to_string())
        .arg("--silent")
        .arg("--ignorecache")
        .arg("--buildpath")
        .arg(buildpath);
    if verbose {
        command.arg("--verbose");
    }
    command
}

fn gaea_swarm_command_preview(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
) -> String {
    let command = gaea_swarm_command(swarm_exe, terrain, node_id, resolution, buildpath, verbose);
    command_preview(&command)
}

fn gaea_swarm_start_process_command(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
    gaea_dir: &Path,
) -> Command {
    let args =
        gaea_swarm_powershell_argument_array(terrain, node_id, resolution, buildpath, verbose);
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $args = @({args}); \
         $p = Start-Process -FilePath '{exe}' -ArgumentList $args -WorkingDirectory '{work}' -WindowStyle Hidden -Wait -PassThru; \
         exit $p.ExitCode",
        exe = escape_powershell_single_quoted(&path_text(swarm_exe)),
        work = escape_powershell_single_quoted(&path_text(gaea_dir)),
    );
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    command
}

fn gaea_swarm_start_process_command_preview(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
    gaea_dir: &Path,
) -> String {
    let command = gaea_swarm_start_process_command(
        swarm_exe, terrain, node_id, resolution, buildpath, verbose, gaea_dir,
    );
    command_preview(&command)
}

fn gaea_swarm_powershell_argument_array(
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
) -> String {
    let mut args = vec![
        "--Filename".to_string(),
        path_text(terrain),
        "--node".to_string(),
        node_id.to_string(),
        "--resolution".to_string(),
        resolution.to_string(),
        "--silent".to_string(),
        "--ignorecache".to_string(),
        "--buildpath".to_string(),
        path_text(buildpath),
    ];
    if verbose {
        args.push("--verbose".to_string());
    }
    args.into_iter()
        .map(|arg| format!("'{}'", escape_powershell_single_quoted(&arg)))
        .collect::<Vec<_>>()
        .join(",")
}

fn recent_swarm_logs(log_dir: &Path, started: SystemTime) -> Result<Vec<PathBuf>, String> {
    if !log_dir.exists() {
        return Ok(Vec::new());
    }
    let mut logs = Vec::new();
    for entry in fs::read_dir(log_dir)
        .map_err(|error| format!("Failed to read '{}': {error}", log_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("Failed to read log entry: {error}"))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if !name.contains("SWARM") || !name.ends_with(".txt") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        if modified >= started {
            logs.push(path);
        }
    }
    logs.sort();
    Ok(logs)
}

fn parse_swarm_log(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let events = text
        .lines()
        .filter_map(parse_swarm_build_event)
        .collect::<Vec<_>>();
    let first_second = events
        .iter()
        .filter_map(|event| event.get("second_of_day").and_then(Value::as_u64))
        .min();
    let last_second = events
        .iter()
        .filter_map(|event| event.get("second_of_day").and_then(Value::as_u64))
        .max();
    let build_elapsed_seconds = first_second
        .zip(last_second)
        .map(|(first, last)| last.saturating_sub(first));
    Ok(json!({
        "path": path,
        "line_count": text.lines().count(),
        "build_event_count": events.len(),
        "build_elapsed_seconds": build_elapsed_seconds,
        "events": events,
    }))
}

fn parse_swarm_build_event(line: &str) -> Option<Value> {
    let time = line.strip_prefix('[')?.get(..8)?;
    let second_of_day = parse_hms_seconds(time)?;
    let event = if line.contains(" - Build Started") {
        "started"
    } else if line.contains(" - Build Finished") {
        "finished"
    } else {
        return None;
    };
    let after_inf = line.split("] INF ").nth(1).unwrap_or(line);
    let node_part = after_inf.split(" - Build ").next().unwrap_or("").trim();
    let node_name = node_part
        .split_once("] ")
        .map(|(_, name)| name.trim())
        .unwrap_or(node_part);
    Some(json!({
        "time": time,
        "second_of_day": second_of_day,
        "node": node_name,
        "event": event,
        "line": line,
    }))
}

fn parse_hms_seconds(value: &str) -> Option<u64> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse::<u64>().ok()?;
    let minute = parts.next()?.parse::<u64>().ok()?;
    let second = parts.next()?.parse::<u64>().ok()?;
    Some(hour * 3600 + minute * 60 + second)
}

fn list_relative_files(root: &Path) -> Result<Vec<Value>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to read '{}': {error}", dir.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read directory entry: {error}"))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| format!("Failed to stat '{}': {error}", path.display()))?;
            if metadata.is_dir() {
                stack.push(path);
            } else {
                files.push(json!({
                    "path": path.strip_prefix(root).unwrap_or(&path),
                    "bytes": metadata.len(),
                }));
            }
        }
    }
    Ok(files)
}

fn extract_jsonish(text: &str) -> Option<String> {
    for (index, ch) in text.char_indices() {
        if ch == '{' || ch == '[' {
            let candidate = text[index..].trim();
            if serde_json::from_str::<Value>(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))
}

fn write_pretty_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
    fs::write(path, text).map_err(|error| format!("Failed to write '{}': {error}", path.display()))
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| format!("Failed to write '{}': {error}", path.display()))
}

fn read_coverage(path: &Path) -> Result<Vec<CoverageRow>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let mut lines = text.lines();
    let headers = lines
        .next()
        .ok_or_else(|| format!("Coverage file '{}' is empty.", path.display()))?
        .split('\t')
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut values = BTreeMap::new();
            for (header, value) in headers.iter().zip(line.split('\t')) {
                values.insert(header.clone(), value.to_string());
            }
            CoverageRow { values }
        })
        .collect())
}

impl CoverageRow {
    fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }
}

fn find_related_summary_files(
    summary_dir: &Path,
    node: &str,
    dossier: Option<&str>,
) -> Result<Vec<String>, String> {
    let node_lower = node.to_ascii_lowercase();
    let mut files = Vec::new();
    for entry in fs::read_dir(summary_dir)
        .map_err(|error| format!("Failed to scan '{}': {error}", summary_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("Failed to read summary entry: {error}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string();
        let name_lower = name.to_ascii_lowercase();
        if name_lower.contains(&node_lower) || dossier.map(|d| d == name).unwrap_or(false) {
            files.push(path.display().to_string());
        }
    }
    files.sort();
    Ok(files)
}

fn split_semicolon_list(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn print_value(as_json: bool, value: &Value) {
    if as_json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
        return;
    }
    print_text_value(value, 0);
}

fn print_text_value(value: &Value, depth: usize) {
    let indent = "  ".repeat(depth);
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                match value {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{indent}{key}:");
                        print_text_value(value, depth + 1);
                    }
                    _ => println!("{indent}{key}: {}", scalar_text(value)),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{indent}-");
                        print_text_value(item, depth + 1);
                    }
                    _ => println!("{indent}- {}", scalar_text(item)),
                }
            }
        }
        _ => println!("{indent}{}", scalar_text(value)),
    }
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn command_preview(command: &Command) -> String {
    let mut parts = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                powershell_env_assignment(
                    key.to_string_lossy().as_ref(),
                    value.to_string_lossy().as_ref(),
                )
            })
        })
        .collect::<Vec<_>>();
    parts.push(command.get_program().to_string_lossy().to_string());
    parts.extend(
        command
            .get_args()
            .map(|arg| quote_arg(&arg.to_string_lossy())),
    );
    parts.join(" ")
}

fn gaea_flywheel_cargo_env_assignment() -> String {
    powershell_env_assignment("CARGO_TARGET_DIR", &path_text(&gaea_flywheel_target_dir()))
}

fn powershell_env_assignment(key: &str, value: &str) -> String {
    format!("$env:{key}='{}';", value.replace('\'', "''"))
}

fn quote_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains(';') || arg.contains('&') {
        format!("'{}'", arg.replace('\'', "''"))
    } else {
        arg.to_string()
    }
}

fn sanitize_filename(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn unix_stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unix_stamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
