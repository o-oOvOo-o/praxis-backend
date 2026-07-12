use crate::{path_text, print_value, read_json, Cli, Context};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TOOLCHAIN_REGISTRY_PATH: &str = "toolchains/reverse_toolchains.json";

#[derive(Debug, Deserialize)]
struct ToolchainRegistry {
    schema_version: u32,
    #[serde(default)]
    cache_subdir: String,
    #[serde(default)]
    archive_subdirs: Vec<String>,
    #[serde(default)]
    tools: Vec<ToolchainSpec>,
}

#[derive(Debug, Deserialize)]
struct ToolchainSpec {
    id: String,
    label: String,
    category: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    env_vars: Vec<String>,
    #[serde(default)]
    path_candidates: Vec<String>,
    #[serde(default)]
    cache_candidates: Vec<String>,
    #[serde(default)]
    which: Vec<String>,
    #[serde(default)]
    version_args: Vec<String>,
    #[serde(default)]
    archive_names: Vec<String>,
    #[serde(default)]
    python_modules: Vec<String>,
    #[serde(default)]
    sync_hint: String,
    #[serde(default)]
    license_policy: String,
    #[serde(default)]
    redistribution: String,
    #[serde(default)]
    notes: String,
}

#[derive(Debug)]
struct ToolchainStatus {
    id: String,
    label: String,
    category: String,
    required: bool,
    found: bool,
    status: String,
    source: Option<String>,
    path: Option<PathBuf>,
    version: Option<String>,
    checked_paths: Vec<PathBuf>,
    archive_hits: Vec<PathBuf>,
    sync_hint: String,
    license_policy: String,
    redistribution: String,
    notes: String,
}

pub(super) fn cmd_toolchain(ctx: &Context, cli: &Cli) -> Result<(), String> {
    match cli
        .flag("mode")
        .unwrap_or("doctor")
        .to_ascii_lowercase()
        .as_str()
    {
        "doctor" | "status" => cmd_toolchain_doctor(ctx, cli),
        "list" | "ls" => cmd_toolchain_list(ctx, cli),
        "sync" | "cache" | "vendor" => cmd_toolchain_sync(ctx, cli),
        other => Err(format!(
            "Unknown toolchain mode '{other}'. Use doctor, list, or sync."
        )),
    }
}

pub(super) fn cmd_toolchain_list(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let registry = load_toolchain_registry(ctx)?;
    let cache_root = toolchain_cache_root(ctx, &registry);
    let archive_dirs = toolchain_archive_dirs(ctx, &registry);
    let tools = registry
        .tools
        .iter()
        .map(|tool| {
            json!({
                "id": tool.id,
                "label": tool.label,
                "category": tool.category,
                "required": tool.required,
                "env_vars": tool.env_vars,
                "path_candidates": tool.path_candidates,
                "cache_candidates": tool.cache_candidates,
                "which": tool.which,
                "version_args": tool.version_args,
                "archive_names": tool.archive_names,
                "python_modules": tool.python_modules,
                "sync_hint": optional_text(&tool.sync_hint),
                "license_policy": optional_text(&tool.license_policy),
                "redistribution": optional_text(&tool.redistribution),
                "notes": optional_text(&tool.notes),
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "mode": "list",
        "schema_version": registry.schema_version,
        "registry_path": toolchain_registry_path(ctx),
        "cache_root": cache_root,
        "archive_dirs": archive_dirs,
        "tool_count": tools.len(),
        "required_count": registry.tools.iter().filter(|tool| tool.required).count(),
        "tools": tools,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

pub(super) fn cmd_toolchain_doctor(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let registry = load_toolchain_registry(ctx)?;
    let cache_root = toolchain_cache_root(ctx, &registry);
    let archive_dirs = toolchain_archive_dirs(ctx, &registry);
    let statuses = collect_toolchain_statuses(ctx, &registry, &cache_root, &archive_dirs);
    let required_missing_count = statuses
        .iter()
        .filter(|status| status.required && !status.found)
        .count();
    let optional_missing_count = statuses
        .iter()
        .filter(|status| !status.required && !status.found)
        .count();
    let archive_available_count = statuses
        .iter()
        .filter(|status| !status.found && !status.archive_hits.is_empty())
        .count();
    let payload = json!({
        "mode": "doctor",
        "schema_version": registry.schema_version,
        "registry_path": toolchain_registry_path(ctx),
        "cache_root": cache_root,
        "archive_dirs": archive_dirs,
        "ready": required_missing_count == 0,
        "required_missing_count": required_missing_count,
        "optional_missing_count": optional_missing_count,
        "archive_available_count": archive_available_count,
        "tools": statuses.iter().map(toolchain_status_value).collect::<Vec<_>>(),
        "recommended_next_commands": toolchain_next_commands(&statuses),
    });
    print_value(cli.json(), &payload);
    if cli.has("strict") && required_missing_count > 0 {
        return Err(format!(
            "Reverse toolchain doctor failed: {required_missing_count} required tool(s) missing."
        ));
    }
    Ok(())
}

pub(super) fn cmd_toolchain_sync(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let registry = load_toolchain_registry(ctx)?;
    let cache_root = toolchain_cache_root(ctx, &registry);
    let archive_dirs = toolchain_archive_dirs(ctx, &registry);
    fs::create_dir_all(&cache_root)
        .map_err(|error| format!("Failed to create '{}': {error}", cache_root.display()))?;
    for dir in &archive_dirs {
        fs::create_dir_all(dir)
            .map_err(|error| format!("Failed to create '{}': {error}", dir.display()))?;
    }
    let statuses = collect_toolchain_statuses(ctx, &registry, &cache_root, &archive_dirs);
    let actions = statuses
        .iter()
        .map(|status| {
            let action = if status.found {
                "already_available"
            } else if !status.archive_hits.is_empty() {
                "archive_available"
            } else if !status.sync_hint.is_empty() {
                "run_sync_hint"
            } else if status.required {
                "manual_required"
            } else {
                "optional_missing"
            };
            json!({
                "id": status.id,
                "label": status.label,
                "required": status.required,
                "action": action,
                "found": status.found,
                "path": status.path,
                "archive_hits": path_values(&status.archive_hits),
                "sync_hint": optional_text(&status.sync_hint),
                "will_execute": false,
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "mode": "sync",
        "offline": cli.has("offline"),
        "repair_requested": cli.has("repair"),
        "executed_external_commands": false,
        "cache_root": cache_root,
        "archive_dirs": archive_dirs,
        "actions": actions,
        "notes": [
            "sync creates canonical cache/archive directories and reports deterministic local repair actions.",
            "external downloads and global installs are never executed implicitly; run the emitted sync_hint command explicitly when wanted."
        ],
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn load_toolchain_registry(ctx: &Context) -> Result<ToolchainRegistry, String> {
    read_json(&toolchain_registry_path(ctx))
}

fn toolchain_registry_path(ctx: &Context) -> PathBuf {
    ctx.devflywheel_dir.join(TOOLCHAIN_REGISTRY_PATH)
}

fn toolchain_cache_root(ctx: &Context, registry: &ToolchainRegistry) -> PathBuf {
    let subdir = if registry.cache_subdir.trim().is_empty() {
        "toolchains"
    } else {
        registry.cache_subdir.trim()
    };
    let path = PathBuf::from(subdir);
    if path.is_absolute() {
        path
    } else {
        ctx.artifact_root.join(path)
    }
}

fn toolchain_archive_dirs(ctx: &Context, registry: &ToolchainRegistry) -> Vec<PathBuf> {
    let subdirs = if registry.archive_subdirs.is_empty() {
        vec!["toolchains/vendor_archives".to_string()]
    } else {
        registry.archive_subdirs.clone()
    };
    let mut dirs = Vec::new();
    for subdir in subdirs {
        let path = PathBuf::from(&subdir);
        if path.is_absolute() {
            dirs.push(path);
        } else {
            dirs.push(ctx.devflywheel_dir.join(&path));
            dirs.push(ctx.artifact_root.join(path));
        }
    }
    unique_paths(dirs)
}

fn collect_toolchain_statuses(
    ctx: &Context,
    registry: &ToolchainRegistry,
    cache_root: &Path,
    archive_dirs: &[PathBuf],
) -> Vec<ToolchainStatus> {
    registry
        .tools
        .iter()
        .map(|tool| toolchain_status(ctx, tool, registry, cache_root, archive_dirs))
        .collect()
}

fn toolchain_status(
    ctx: &Context,
    tool: &ToolchainSpec,
    registry: &ToolchainRegistry,
    cache_root: &Path,
    archive_dirs: &[PathBuf],
) -> ToolchainStatus {
    let mut candidates = Vec::<(String, PathBuf)>::new();
    candidates.extend(env_toolchain_candidates(ctx, tool, registry, cache_root));
    candidates.extend(explicit_toolchain_candidates(
        ctx, tool, registry, cache_root,
    ));
    candidates.extend(cache_toolchain_candidates(ctx, tool, registry, cache_root));
    candidates.extend(which_toolchain_candidates(tool));
    let mut checked_paths = Vec::new();
    let mut found = None;
    for (source, path) in unique_source_paths(candidates) {
        checked_paths.push(path.clone());
        if toolchain_candidate_ready(tool, &path) {
            found = Some((source, path));
            break;
        }
    }
    let archive_hits = toolchain_archive_hits(ctx, tool, registry, cache_root, archive_dirs);
    let (source, path, version) = if let Some((source, path)) = found {
        let version = toolchain_version(&path, &tool.version_args);
        (Some(source), Some(path), version)
    } else {
        (None, None, None)
    };
    let found = path.is_some();
    let status = if found {
        "found"
    } else if !archive_hits.is_empty() {
        "archive_available"
    } else if tool.required {
        "missing_required"
    } else {
        "missing_optional"
    }
    .to_string();
    ToolchainStatus {
        id: tool.id.clone(),
        label: tool.label.clone(),
        category: tool.category.clone(),
        required: tool.required,
        found,
        status,
        source,
        path,
        version,
        checked_paths,
        archive_hits,
        sync_hint: tool.sync_hint.clone(),
        license_policy: tool.license_policy.clone(),
        redistribution: tool.redistribution.clone(),
        notes: tool.notes.clone(),
    }
}

fn env_toolchain_candidates(
    ctx: &Context,
    tool: &ToolchainSpec,
    registry: &ToolchainRegistry,
    cache_root: &Path,
) -> Vec<(String, PathBuf)> {
    let mut paths = Vec::new();
    for env_var in &tool.env_vars {
        let Some(value) = env::var_os(env_var) else {
            continue;
        };
        for base in env::split_paths(&value) {
            let expanded =
                expand_toolchain_path(ctx, registry, cache_root, &base.to_string_lossy());
            for path in toolchain_env_path_variants(tool, expanded) {
                paths.push((format!("env:{env_var}"), path));
            }
        }
    }
    paths
}

fn toolchain_env_path_variants(tool: &ToolchainSpec, base: PathBuf) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    match tool.id.as_str() {
        "java-jdk" => paths.push(base.join("bin").join("java.exe")),
        "ghidra" => paths.push(base.join("support").join("analyzeHeadless.bat")),
        "gaea-install" => paths.push(base.join("Gaea.Swarm.exe")),
        _ => {}
    }
    paths.push(base);
    paths
}

fn explicit_toolchain_candidates(
    ctx: &Context,
    tool: &ToolchainSpec,
    registry: &ToolchainRegistry,
    cache_root: &Path,
) -> Vec<(String, PathBuf)> {
    tool.path_candidates
        .iter()
        .flat_map(|candidate| {
            expand_toolchain_candidate(ctx, registry, cache_root, candidate)
                .into_iter()
                .map(|path| ("path_candidate".to_string(), path))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn cache_toolchain_candidates(
    ctx: &Context,
    tool: &ToolchainSpec,
    registry: &ToolchainRegistry,
    cache_root: &Path,
) -> Vec<(String, PathBuf)> {
    tool.cache_candidates
        .iter()
        .flat_map(|candidate| {
            let raw = cache_root.join(candidate).display().to_string();
            expand_toolchain_candidate(ctx, registry, cache_root, &raw)
                .into_iter()
                .map(|path| ("cache_candidate".to_string(), path))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn which_toolchain_candidates(tool: &ToolchainSpec) -> Vec<(String, PathBuf)> {
    tool.which
        .iter()
        .flat_map(|command| {
            which_candidates(command)
                .into_iter()
                .map(|path| (format!("path:{command}"), path))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn toolchain_archive_hits(
    ctx: &Context,
    tool: &ToolchainSpec,
    registry: &ToolchainRegistry,
    cache_root: &Path,
    archive_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut hits = Vec::new();
    for archive_dir in archive_dirs {
        for archive_name in &tool.archive_names {
            let pattern = archive_dir.join(archive_name).display().to_string();
            hits.extend(
                expand_toolchain_candidate(ctx, registry, cache_root, &pattern)
                    .into_iter()
                    .filter(|path| path.exists()),
            );
        }
    }
    unique_paths(hits)
}

fn expand_toolchain_candidate(
    ctx: &Context,
    registry: &ToolchainRegistry,
    cache_root: &Path,
    candidate: &str,
) -> Vec<PathBuf> {
    let expanded = expand_toolchain_path(ctx, registry, cache_root, candidate);
    if expanded.to_string_lossy().contains('*') || expanded.to_string_lossy().contains('?') {
        let paths = expand_wildcard_path(&expanded);
        if paths.is_empty() {
            vec![expanded]
        } else {
            paths
        }
    } else {
        vec![expanded]
    }
}

fn expand_toolchain_path(
    ctx: &Context,
    _registry: &ToolchainRegistry,
    cache_root: &Path,
    text: &str,
) -> PathBuf {
    let mut out = text
        .replace("{root}", &path_text(&ctx.root))
        .replace("{devflywheel_dir}", &path_text(&ctx.devflywheel_dir))
        .replace("{artifact_root}", &path_text(&ctx.artifact_root))
        .replace("{cache_root}", &path_text(cache_root));
    for name in [
        "USERPROFILE",
        "LOCALAPPDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "PROGRAMW6432",
        "WINDIR",
        "SYSTEMROOT",
        "GHOST1_ROOT",
        "C3D_DEVFLYWHEEL_ARTIFACT_ROOT",
        "GHOST1_DEVFLYWHEEL_ARTIFACT_ROOT",
    ] {
        if let Some(value) = env::var_os(name) {
            out = replace_case_insensitive(&out, &format!("%{name}%"), &value.to_string_lossy());
        }
    }
    PathBuf::from(out)
}

fn expand_wildcard_path(pattern: &Path) -> Vec<PathBuf> {
    let mut prefixes = vec![PathBuf::new()];
    for component in pattern.components() {
        let text = component.as_os_str().to_string_lossy().to_string();
        if text.contains('*') || text.contains('?') {
            let mut next = Vec::new();
            for prefix in &prefixes {
                let Ok(entries) = fs::read_dir(prefix) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if wildcard_match(&text, &name) {
                        next.push(entry.path());
                    }
                }
            }
            prefixes = next;
        } else {
            for prefix in &mut prefixes {
                prefix.push(component.as_os_str());
            }
        }
        if prefixes.is_empty() {
            break;
        }
    }
    unique_paths(prefixes)
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let mut p = 0usize;
    let mut t = 0usize;
    let mut star = None;
    let mut star_text = 0usize;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            star_text = t;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            star_text += 1;
            t = star_text;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn which_candidates(command: &str) -> Vec<PathBuf> {
    let command_path = PathBuf::from(command);
    if command_path.components().count() > 1 {
        return vec![command_path]
            .into_iter()
            .filter(|path| path.exists())
            .collect();
    }
    let Some(path_var) = env::var_os("PATH") else {
        return Vec::new();
    };
    let pathexts = if cfg!(windows) {
        env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.BAT;.CMD;.COM".to_string())
    } else {
        String::new()
    };
    let ext_candidates = if command_path.extension().is_some() || !cfg!(windows) {
        vec![String::new()]
    } else {
        pathexts
            .split(';')
            .filter(|ext| !ext.trim().is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let mut paths = Vec::new();
    for dir in env::split_paths(&path_var) {
        if ext_candidates.len() == 1 && ext_candidates[0].is_empty() {
            paths.push(dir.join(command));
        } else {
            for ext in &ext_candidates {
                paths.push(dir.join(format!("{command}{ext}")));
            }
        }
    }
    unique_paths(paths.into_iter().filter(|path| path.exists()).collect())
}

fn toolchain_version(path: &Path, args: &[String]) -> Option<String> {
    if args.is_empty() || path.is_dir() {
        return None;
    }
    let output = Command::new(path).args(args).output().ok()?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if text.trim().is_empty() {
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn toolchain_candidate_ready(tool: &ToolchainSpec, path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    if tool.python_modules.is_empty() {
        return true;
    }
    python_modules_available(path, &tool.python_modules)
}

fn python_modules_available(python: &Path, modules: &[String]) -> bool {
    modules.iter().all(|module| {
        Command::new(python)
            .args(["-c", &format!("import {module}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

fn toolchain_status_value(status: &ToolchainStatus) -> Value {
    json!({
        "id": status.id,
        "label": status.label,
        "category": status.category,
        "required": status.required,
        "found": status.found,
        "status": status.status,
        "source": status.source,
        "path": status.path,
        "version": status.version,
        "checked_paths": path_values(&status.checked_paths),
        "archive_hits": path_values(&status.archive_hits),
        "sync_hint": optional_text(&status.sync_hint),
        "license_policy": optional_text(&status.license_policy),
        "redistribution": optional_text(&status.redistribution),
        "notes": optional_text(&status.notes),
    })
}

fn toolchain_next_commands(statuses: &[ToolchainStatus]) -> Vec<String> {
    statuses
        .iter()
        .filter(|status| !status.found && status.required)
        .filter_map(|status| {
            if !status.sync_hint.is_empty() {
                Some(status.sync_hint.clone())
            } else {
                Some(format!("Install or cache {} ({})", status.label, status.id))
            }
        })
        .collect()
}

fn unique_source_paths(paths: Vec<(String, PathBuf)>) -> Vec<(String, PathBuf)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (source, path) in paths {
        let key = path_text(&path).to_ascii_lowercase();
        if seen.insert(key) {
            out.push((source, path));
        }
    }
    out
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for path in paths {
        let key = path_text(&path).to_ascii_lowercase();
        if seen.insert(key) {
            out.push(path);
        }
    }
    out
}

fn path_values(paths: &[PathBuf]) -> Vec<Value> {
    paths.iter().map(|path| json!(path)).collect()
}

fn optional_text(text: &str) -> Value {
    if text.trim().is_empty() {
        Value::Null
    } else {
        Value::String(text.to_string())
    }
}

fn replace_case_insensitive(text: &str, needle: &str, replacement: &str) -> String {
    let text_lower = text.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut output = String::new();
    let mut start = 0usize;
    let mut search_start = 0usize;
    while let Some(offset) = text_lower[search_start..].find(&needle_lower) {
        let index = search_start + offset;
        output.push_str(&text[start..index]);
        output.push_str(replacement);
        search_start = index + needle.len();
        start = search_start;
    }
    output.push_str(&text[start..]);
    output
}
