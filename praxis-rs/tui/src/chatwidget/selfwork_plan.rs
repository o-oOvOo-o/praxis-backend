use std::path::Path;
use std::path::PathBuf;

use crate::diff_render::display_path_for;
use crate::text_formatting::truncate_text;
pub(super) use praxis_app_core::selfwork::SELFWORK_PLAN_SCAN_LIMIT;
pub(super) use praxis_app_core::selfwork::SELFWORK_STALL_LIMIT;
pub(super) use praxis_app_core::selfwork::SelfworkCommand;
pub(super) use praxis_app_core::selfwork::SelfworkPlanInspection;
pub(super) use praxis_app_core::selfwork::SelfworkRuntimeState;
pub(super) use praxis_app_core::selfwork::collect_selfwork_plan_paths;
pub(super) use praxis_app_core::selfwork::inspect_selfwork_plan;
pub(super) use praxis_app_core::selfwork::parse_selfwork_command;

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
