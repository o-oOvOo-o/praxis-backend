use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;

use crate::diff_render::display_path_for;
use crate::text_formatting::truncate_text;
pub(super) use praxis_app_core::selfwork::{
    SELFWORK_PLAN_SCAN_LIMIT, SELFWORK_STALL_LIMIT, SelfworkPlanAdvance, SelfworkPlanInspection,
    SelfworkRuntimeState, inspect_selfwork_plan, selfwork_prompt,
};

pub(super) const SELFWORK_PICKER_VIEW_ID: &str = "selfwork-plan-selection";
pub(super) const SELFWORK_USAGE: &str =
    "Use /selfwork to choose a markdown plan, or /selfwork start <plan.md> (alias: /loop).";
const SELFWORK_PLAN_PREVIEW_LINE_LIMIT: usize = 6;
const SELFWORK_PLAN_PREVIEW_WIDTH: usize = 88;

#[derive(Debug, Clone)]
pub(super) struct SelfworkPlanCandidate {
    pub(super) path: PathBuf,
    pub(super) display_path: String,
    pub(super) description: String,
    pub(super) selected_description: String,
    pub(super) search_value: String,
}

#[derive(Debug, Clone)]
pub(super) struct SelfworkPlanDiscovery {
    pub(super) root: PathBuf,
    pub(super) candidates: Vec<SelfworkPlanCandidate>,
    pub(super) truncated: bool,
}

pub(super) fn selfwork_search_root(current_cwd: Option<&Path>, config_cwd: &Path) -> PathBuf {
    current_cwd.unwrap_or(config_cwd).to_path_buf()
}

pub(super) fn discover_selfwork_plan_candidates(
    root: PathBuf,
) -> Result<SelfworkPlanDiscovery, String> {
    let (paths, truncated) = collect_selfwork_plan_paths(root.as_path())?;
    let candidates = paths
        .into_iter()
        .filter_map(|path| build_selfwork_plan_candidate(root.as_path(), path).ok())
        .collect();
    Ok(SelfworkPlanDiscovery {
        root,
        candidates,
        truncated,
    })
}

pub(super) fn resolve_selfwork_plan_path(
    raw: &str,
    current_cwd: Option<&Path>,
    config_cwd: &Path,
) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SELFWORK_USAGE.to_string());
    }

    let unquoted = if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    let candidate = PathBuf::from(unquoted);
    let resolved = if candidate.is_absolute() {
        candidate
    } else {
        selfwork_search_root(current_cwd, config_cwd).join(candidate)
    };

    if !resolved.exists() {
        return Err(format!("Selfwork plan not found: {}", resolved.display()));
    }
    if !resolved.is_file() {
        return Err(format!(
            "Selfwork plan must be a file: {}",
            resolved.display()
        ));
    }

    Ok(resolved)
}

fn collect_selfwork_plan_paths(root: &Path) -> Result<(Vec<PathBuf>, bool), String> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut paths = Vec::new();
    let mut truncated = false;

    while let Some(dir) = pending.pop_front() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|err| format!("Failed to scan markdown plans in {}: {err}", dir.display()))?;
        let mut children = entries
            .filter_map(Result::ok)
            .collect::<Vec<std::fs::DirEntry>>();
        children.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

        for entry in children {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                if should_descend_selfwork_dir(&name) {
                    pending.push_back(path);
                }
                continue;
            }
            if !file_type.is_file() || !is_markdown_plan_file(&name) {
                continue;
            }
            paths.push(path);
            if paths.len() >= SELFWORK_PLAN_SCAN_LIMIT {
                truncated = true;
                break;
            }
        }

        if truncated {
            break;
        }
    }

    paths.sort_by_key(|path| selfwork_candidate_sort_key(root, path));
    Ok((paths, truncated))
}

fn build_selfwork_plan_candidate(
    root: &Path,
    path: PathBuf,
) -> Result<SelfworkPlanCandidate, String> {
    let display_path = display_path_for(path.as_path(), root);
    let inspection = inspect_selfwork_plan(path.as_path())?;
    let contents = std::fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read selfwork plan {}: {err}", path.display()))?;
    let status = selfwork_status_summary(&inspection);
    let preview = selfwork_preview_from_contents(&contents);
    Ok(SelfworkPlanCandidate {
        path,
        display_path: display_path.clone(),
        description: status.clone(),
        selected_description: format!("Path: {display_path}\nStatus: {status}\n\n{preview}"),
        search_value: format!("{display_path}\n{status}\n{preview}"),
    })
}

fn should_descend_selfwork_dir(name: &str) -> bool {
    !matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | ".praxis"
            | ".codex"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | "coverage"
            | ".next"
            | ".nuxt"
    )
}

fn is_markdown_plan_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

fn selfwork_candidate_sort_key(root: &Path, path: &Path) -> (u8, usize, String) {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name_rank = if file_name == "plan.md" {
        0
    } else if matches!(file_name.as_str(), "selfwork.md" | "loop.md") {
        1
    } else if file_name.contains("plan") {
        2
    } else if file_name.contains("todo") || file_name.contains("task") {
        3
    } else {
        4
    };
    let depth = path
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.components().count())
        .unwrap_or(usize::MAX);
    let display = display_path_for(path, root).to_ascii_lowercase();
    (name_rank, depth, display)
}

fn selfwork_status_summary(inspection: &SelfworkPlanInspection) -> String {
    if inspection.complete {
        "Looks complete".to_string()
    } else if inspection.checklist_total > 0 {
        format!(
            "{} unfinished of {} checklist items",
            inspection.checklist_unchecked, inspection.checklist_total
        )
    } else {
        "Markdown plan".to_string()
    }
}

fn selfwork_preview_from_contents(contents: &str) -> String {
    let preview_lines = contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(SELFWORK_PLAN_PREVIEW_LINE_LIMIT)
        .map(|line| truncate_text(line, SELFWORK_PLAN_PREVIEW_WIDTH))
        .collect::<Vec<_>>();
    if preview_lines.is_empty() {
        "(empty markdown file)".to_string()
    } else {
        preview_lines.join("\n")
    }
}
