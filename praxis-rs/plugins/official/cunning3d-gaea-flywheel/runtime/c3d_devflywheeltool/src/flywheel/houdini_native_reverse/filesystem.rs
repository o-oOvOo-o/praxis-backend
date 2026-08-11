fn should_run_ghidra_analysis(project_exists: bool, deep: bool, reanalyze: bool) -> bool {
    (!project_exists && deep) || (project_exists && reanalyze)
}

fn resolve_first_file(candidates: &[Option<PathBuf>]) -> Option<PathBuf> {
    candidates
        .iter()
        .flatten()
        .find(|path| path.is_file())
        .cloned()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Failed to open '{}': {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to hash '{}': {error}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn houdini_command_preview(command: &Command) -> Vec<String> {
    std::iter::once(command.get_program().to_string_lossy().to_string())
        .chain(
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().to_string()),
        )
        .collect()
}

fn collect_relative_files(root: &Path) -> Vec<String> {
    fn visit(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, out);
            } else if let Ok(relative) = path.strip_prefix(root) {
                out.push(path_text(relative));
            }
        }
    }
    let mut files = Vec::new();
    visit(root, root, &mut files);
    files.sort();
    files
}
