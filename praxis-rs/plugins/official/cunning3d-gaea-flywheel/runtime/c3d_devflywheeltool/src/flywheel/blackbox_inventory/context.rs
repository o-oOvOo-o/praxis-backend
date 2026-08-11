#[derive(Debug)]
struct Context {
    root: PathBuf,
    tools_gaea: PathBuf,
    gaea_decompiled_root: PathBuf,
    summary_dir: PathBuf,
    harness_project: PathBuf,
    harness_exe: PathBuf,
    cunning_core_manifest: PathBuf,
    gaea_flywheel_target_dir: PathBuf,
    cunning_core_target_debug_dir: PathBuf,
    cunning_core_target_release_dir: PathBuf,
    devflywheel_dir: PathBuf,
    artifact_root: PathBuf,
}

impl Context {
    fn discover() -> Result<Self, String> {
        let root = env::var_os("CUNNING3D_ROOT")
            .map(PathBuf::from)
            .and_then(normalize_cunning3d_root)
            .or_else(find_root_from_current_dir)
            .ok_or_else(|| {
                "Could not discover the Cunning3D repository root. Run from the repository or set CUNNING3D_ROOT.".to_string()
            })?;
        let local_root = root.join(".local");
        let tools_gaea = local_root.join("gaea");
        let gaea_decompiled_root = tools_gaea.join("decompiled");
        let summary_dir = gaea_decompiled_root.join("_summary");
        let harness_root = tools_gaea.join("harness");
        let harness_project = harness_root.join("GaeaReverseHarness.csproj");
        let harness_exe = harness_root
            .join("bin")
            .join("Debug")
            .join("net8.0-windows")
            .join("GaeaReverseHarness.exe");
        let cunning_core_manifest = root.join("crates").join("cunning_core").join("Cargo.toml");
        let gaea_flywheel_target_dir = gaea_flywheel_target_dir();
        let cunning_core_target_debug_dir = gaea_flywheel_target_dir.join("debug");
        let cunning_core_target_release_dir = gaea_flywheel_target_dir.join("release");
        let devflywheel_dir = discover_devflywheel_dir(&root)?;
        let artifact_root = env::var_os("C3D_DEVFLYWHEEL_ARTIFACT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| local_root.join("gaea-flywheel").join("artifacts"));
        Ok(Self {
            root,
            tools_gaea,
            gaea_decompiled_root,
            summary_dir,
            harness_project,
            harness_exe,
            cunning_core_manifest,
            gaea_flywheel_target_dir,
            cunning_core_target_debug_dir,
            cunning_core_target_release_dir,
            devflywheel_dir,
            artifact_root,
        })
    }
}

fn find_root_from_current_dir() -> Option<PathBuf> {
    let current = env::current_dir().ok()?;
    for dir in current.ancestors() {
        if let Some(root) = normalize_cunning3d_root(dir.to_path_buf()) {
            return Some(root);
        }
    }
    None
}

fn normalize_cunning3d_root(candidate: PathBuf) -> Option<PathBuf> {
    // Resolve only the canonical repository that directly owns cunning_core.
    if candidate
        .join("crates")
        .join("cunning_core")
        .join("Cargo.toml")
        .is_file()
    {
        return Some(candidate);
    }
    None
}

fn discover_devflywheel_dir(_root: &Path) -> Result<PathBuf, String> {
    let dir = env::var_os("C3D_DEVFLYWHEEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if dir.join("Cargo.toml").exists() && dir.join(LEDGER_PATH).exists() {
        Ok(dir)
    } else {
        Err(format!(
            "The Praxis Gaea flywheel runtime is incomplete at '{}'. Reinstall the cunning3d-gaea-flywheel plugin or set C3D_DEVFLYWHEEL_DIR.",
            dir.display()
        ))
    }
}

fn gaea_flywheel_target_dir() -> PathBuf {
    env::var_os("C3D_GAEA_FLYWHEEL_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAEA_FLYWHEEL_TARGET_DIR))
}
